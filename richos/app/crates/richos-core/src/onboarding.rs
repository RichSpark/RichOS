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
//! # THE INTERVIEW ENDS WITH A TEAM PROPOSED, AND HIS "GO" HIRES IT
//!
//! The CEO, 2026-10-13: *"Why the fuck does Rich not already automatically auto-hire everyone
//! he deems to be needed???"*, and then the ending in his own words: Rich names the five
//! teammates every install starts with (Clark, Dean, Frank, Pierce, Reed —
//! `engine_profile::ALWAYS_ACTIVE`), lists the new ones the interview calls for, and ends *"Say
//! go and I'll get them hired now."* The skill carries that message word for word.
//!
//! Until 2026-10-13 this section said staffing was declared and never attempted, because the
//! front desk has no way to hire and an early measurement (cells B2/B3,
//! `docs/verification/onboarding-honesty-2026-09-06/`) found a spawn of Dean failing. Both are
//! still true of the FRONT DESK, which is why the skill tells it to spawn nobody. What changed
//! is the install: since the proto-teammate shelf (richos-hq
//! `docs/plans/2026-10-06-proto-teammate-shelf.md`) the back end hires — Dean fits a shelf
//! teammate or writes a new one, `richos_work.team` saves it into `<app data>/team/`, and every
//! later lease registers it. So his go is an assignment like any other, and the honesty rule
//! moves with it: a teammate is named as hired only once the hiring has reported it saved.

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

/// **The CEO's sentence about the team** (richos-hq `docs/ceo-input/2026-10-13_1/feedback.md`,
/// "Change the copy in the onboarding paragraph"), exactly as he wrote it. Rich says it in the
/// first minute of the interview; [`OFFER_BLOCK`] and the interview skill both carry it.
pub const HIRING_SENTENCE: &str = "And if you describe specialists you'd like me to have on my AI team, I'll add (\"hire\") them once we finish this onboarding interview.";

/// THE OFFER, issued in the priming turn when a company has nothing on file.
///
/// It is written as an instruction to Rich rather than as copy to read out, because the
/// interview is a conversation and a script read aloud is a form. The onboarding document's §2
/// makes the same call: the only new user-facing surface it proposes is a sheet that makes the
/// offer *visible*, and the conversation itself stays a conversation.
///
/// Four things it must contain, and each is here because leaving it out has a named cost:
///
/// 1. **It is his call, and "not now" is a real answer.** Onboarding §4: alone, nobody reminds
///    him that deferral is honest, so an unanswered question reads as a failure to answer. Rich
///    has to say it unprompted, in the first minute.
/// 2. **The team is hired when the interview ends.** The CEO's own sentence, word for word
///    ([`HIRING_SENTENCE`]); the module doc says how his go becomes a hire.
/// 3. **He must not be sold a corpus he will not get.** Onboarding §6: his own corpus compiles
///    to 168 objects against the demo's 7,500 and a new customer's compiles to zero. Any copy
///    implying "answer these and watch your company appear" is a lie the product cannot cover.
/// 4. **Never fabricate.** An unanswered stage stays unanswered on the page.
pub const OFFER_BLOCK: &str = "\
ONBOARDING — you have nothing on file about this company, and that is a fact about this \
install rather than about him. He has not been asked yet.\n\
So offer, once, in your own words and at a natural moment: you can spend about twenty minutes \
asking about his business — what it does, who it is for, and how he wants to work — and you \
will write the answers down and use them from then on. If he is in the middle of something, \
finish that first and offer afterwards. Do not ask the questions until he says yes.\n\
When he says yes, use the skill named 'bootstrap-interview'. It carries the questions, their \
order and what to do with the answers; do not improvise a substitute for it.\n\
Four things are true while you do this, and he needs to hear all four from you:\n\
  - Stopping partway is fine and so is 'not sure yet'. Say that in the first minute, before he \
has to wonder. An unanswered question stays unanswered — never fill one in to make the session \
look finished.\n\
  - Say this sentence about his team, word for word: And if you describe specialists you'd like \
me to have on my AI team, I'll add (\"hire\") them once we finish this onboarding interview.\n\
  - Answering questions will not make his own data appear on the home screen. What is there is a \
worked example. Say so if he asks, and do not imply otherwise if he does not.\n\
  - If he would rather not do this now, say that is fine and let it go. Do not raise it again \
in this conversation. Call mcp__richos_onboarding__decline_onboarding only when he explicitly \
says he does not want this interview now in response to this offer. Never infer that from an \
unrelated 'not now'. Report a declined state only after the tool confirms it.\n\n";

/// What Rich is told when he was already asked and said no.
///
/// Without it he would arrive at every company switch knowing nothing and being told to offer
/// again — the nag M5 names. With it he knows the question was put and answered.
pub const DECLINED_BLOCK: &str = "\
ONBOARDING — you have nothing on file about this company because he was offered the twenty \
minutes of questions and said not now. That was a real answer and it stands. Do not offer \
again unless he brings it up; if he does, the skill named 'bootstrap-interview' is still there. \
Meanwhile, do not guess at facts about his business — ask him, the way you would ask about \
anything you were not told.\n\n";

/// A resumption offers the remaining questions without pretending the saved answers are absent.
pub const RESUME_BLOCK: &str = "ONBOARDING — this company's interview is partly complete. \
The notes above contain answers already saved. Offer to pick up the remaining questions once \
at a natural moment, using rich-skills:bootstrap-interview if he agrees. Never re-ask questions \
already answered. If he explicitly declines this offer, use mcp__richos_onboarding__decline_onboarding \
and leave the saved answers intact.\n\n";

/// What Rich is told when the file exists and cannot be used.
///
/// The only state that needs somebody to act, so it is the only one that says so. It never
/// carries the file's contents — an over-budget file's whole point is that it does not go in.
pub const UNUSABLE_BLOCK: &str = "\
ONBOARDING — there is a file of notes about this company and it could not be used, so you are \
working without it. Do not guess at what it said. If he asks about it, tell him plainly that \
his notes about this company could not be read and that whoever set RichOS up will need to look \
at it.\n\n";

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
         This absolute path is supplied by RichOS. Never guess a different folder. \
         Use mcp__richos_onboarding__save_company_notes for interview notes and \
         mcp__richos_onboarding__decline_onboarding for an explicit decline of the interview. \
         Those tools are bound to this company by the app; do not write the notes with general \
         file or shell tools. A successful save means RichOS read the file back and verified it.\n\n"
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
        assert!(block.contains("he has not been asked yet") || block.contains("He has not been asked yet"));
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
        assert!(block.contains("Do not offer again"), "{block}");
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

    /// The CEO's copy, 2026-10-13, word for word in the offer, and nothing left of the old line
    /// that told him Rich could not hire anyone.
    #[test]
    fn the_offer_carries_his_sentence_and_never_says_rich_cannot_hire() {
        let flat = OFFER_BLOCK.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(flat.contains(HIRING_SENTENCE), "{flat}");
        for gone in ["cannot hire", "not hired", "staffed"] {
            assert!(!flat.contains(gone), "the offer still says {gone:?}: {flat}");
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
        assert!(OFFER_BLOCK.contains("in the first minute"));
        assert!(OFFER_BLOCK.contains("never fill one in"));
    }

    /// Onboarding §6: the demo stays, and the interview must not be sold on the promise that
    /// answering questions makes his own data appear.
    #[test]
    fn the_offer_refuses_to_promise_his_data_on_the_home_screen() {
        assert!(OFFER_BLOCK.contains("will not make his own data appear on the home screen"));
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

    /// **The ending the CEO wrote, 2026-10-13**: the five teammates every install starts with,
    /// the new ones the interview calls for, and *"Say go and I'll get them hired now."* His
    /// go is written down as one hiring assignment, and nobody is named as hired before the
    /// hiring reports them saved.
    #[test]
    fn the_interview_ends_by_proposing_the_team_and_hires_it_on_his_go() {
        let s = interview_skill();
        assert!(s.contains(HIRING_SENTENCE), "his first-minute sentence, word for word");
        assert!(s.contains("At the beginning, I already had these teammates on my AI team: Clark (Senior Researcher), \
            Dean (HR Director for our RichOS team; he hires new AI teammates when needed), Frank (Expert Advisor/Devil's \
            Advocate; one of our most valuable teammates because he helps us uncover blind spots), Pierce (Brief Checker; \
            he's vital in helping me avoid errors when briefing my AI teammates) and Reed (Source-Reading & \
            Knowledge-Extraction Specialist). But based on what you told me here, I think we also need to add the \
            following AI teammates to my team:"), "his opening of the team message, word for word");
        assert!(s.contains("And we can expand/change this team any time as needed. Say go and I'll get them hired now."));
        // The five he names are the five every install registers, so the opening line is true.
        for name in crate::engine_profile::ALWAYS_ACTIVE {
            let shown = format!("{}{} (", name[..1].to_uppercase(), &name[1..]);
            assert!(s.contains(&shown), "{name} is on every team and in his sentence");
        }
        assert!(s.contains("When he says go, hire them at once."));
        assert!(s.contains("Write it down with the assignment register as a task"));
        // The hiring task is a TITLE, and the register keeps a title as one line of at most 160
        // characters (`assignment::sanitize_title`). The walk of 2026-10-13 had Rich write each
        // hire with a line about them: the title was cut after the first teammate ("…; and…")
        // and the back end had to ask him who the second one was. So the skill asks for names
        // and job titles only, and splits a longer list; this ties its number to the code's.
        assert!(s.contains("names and job titles from your list and nothing else"));
        assert!(s.contains("one line of at most 160 characters"));
        let at_cap = "a".repeat(160);
        assert_eq!(crate::assignment::sanitize_title(&at_cap).unwrap(), at_cap, "160 characters are kept whole");
        assert!(crate::assignment::sanitize_title(&format!("{} b", "a".repeat(159))).unwrap().ends_with('…'),
                "161 characters are cut, so the skill's 160 is the register's own limit");
        let example = "Hire these AI teammates onto my team: Nora (Marketing Lead), Mark (Backend Engineer)";
        assert!(s.contains(example) && example.chars().count() <= 160, "the skill's own example fits the line");
        assert!(s.contains("Name someone as hired only after the hiring has reported them saved."));
        assert!(!s.contains("cannot hire"), "the old line is gone");
    }

    /// The ready-made names the skill offers are exactly the shelf this engine ships, so Rich
    /// never proposes a ready-made teammate the back end cannot find, nor misses one it has.
    #[test]
    fn the_ready_made_names_in_the_interview_are_the_shelf() {
        let s = interview_skill();
        let shelf = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().join("engine/team/shelf");
        let mut names: Vec<String> = std::fs::read_dir(&shelf).unwrap()
            .filter_map(|e| e.unwrap().path().file_stem().map(|s| s.to_string_lossy().to_string())).collect();
        names.sort();
        assert!(!names.is_empty());
        let start = "use that teammate's name: ";
        let listed = &s[s.find(start).expect("the ready-made list") + start.len()..];
        let listed = &listed[..listed.find("For any other role").expect("the end of the list")];
        let shown: Vec<&str> = listed.split(", ").map(|item| item.split(" (").next().unwrap().trim()).collect();
        assert_eq!(shown.len(), names.len(), "{listed}");
        for name in names {
            let title = format!("{}{} (", name[..1].to_uppercase(), &name[1..]);
            assert!(listed.contains(&title), "the shelf's {name} is missing from the interview's list");
        }
    }

    /// The engine's interview ends by spawning Dean. This one must not: the front desk spawns
    /// nobody, and his go becomes an assignment. Dean is named now (he is one of the five on
    /// every team, in the CEO's own sentence), so the check is on the spawning words.
    #[test]
    fn the_interview_instructs_no_spawn_and_no_delegation() {
        let lowered = interview_skill().to_lowercase();
        for word in ["subagent", "spawn", "delegate", "isolation"] {
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
