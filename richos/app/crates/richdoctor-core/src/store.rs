//! **SAVED ON THIS MAC: DRAFTS, APPROVED SNAPSHOTS, SETTINGS** (PRD §2.6, §6, §8; round 39 §6).
//!
//! RichDoctor's own data folder, `~/Library/Application Support/com.richos.richdoctor/`
//! (owner-only, PRD §3: its own small settings file and data directory, nothing shared with
//! RichOS):
//!
//! ```text
//! settings.json                 login preference, first-run explanation shown, installation id
//! drafts/<draft id>/draft.json  one report: description, conversation, report, approval, delivery
//! drafts/<draft id>/evidence/   its sanitized evidence (crate::evidence)
//! drafts/<draft id>/scope.json  the incident scope the support server reads
//! sent/<report id>.json         after 30 days, all that is kept of a submitted report: its link
//! ```
//!
//! Every write is atomic and owner-only ([`crate::util::write_private`]). A failed write is
//! reported, never hidden: the window then says "Not saved on this Mac" and keeps the work in
//! memory ([`SaveError`]). Unfinished reports are kept until sent or deleted; a submitted
//! report's local copy is removed 30 days after sending and its link stays ([`Store::expire`]).
//! Quitting during a send erases nothing: the approved report is saved before the request.
use crate::delivery::{Approved, State};
use crate::report::Report;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

/// A submitted report's local copy lives this long (PRD §6).
pub const SUBMITTED_KEPT_MS: u64 = 30 * 24 * 3600 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Settings {
    /// Whether the person wants RichDoctor to start at login. macOS's own answer is what the
    /// menu shows; this is the person's choice, applied at first run and kept.
    #[serde(default = "yes")]
    pub start_at_login: bool,
    #[serde(default)]
    pub first_run_done: bool,
    /// A random id, the relay's weak rate-limit key only (never authentication).
    #[serde(default)]
    pub installation_id: String,
}
fn yes() -> bool {
    true
}

/// One line of the conversation as the window shows it again after a restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Entry {
    /// The person's own words (their description or an answer).
    Person { text: String },
    /// RichDoctor's words (what Claude said).
    Doctor { text: String },
    /// A question, with its choices.
    Question { text: String, choices: Vec<String> },
    /// A finished step: "Checked what happened when RichOS last opened".
    Done { text: String },
    /// A plain note in the conversation (canceled, screenshot added).
    Note { text: String },
    /// Claude Code could not help: the observed cause.
    AiFailed { failure: crate::adapter::Failure },
}

/// Where a draft is in its journey.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    Describe,
    Investigating,
    Review,
    Approved,
}

/// One approved screenshot: the final, metadata-free image the person approved with its covers
/// burned in (PRD §6, [`crate::image_profile::finalize`]), stored as `shots/<id>` in the draft's
/// folder (owner-only). Its bytes are what is shown, hashed, delivered and attached to email.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shot {
    pub id: String,
    pub sha256: String,
    pub covers: u32,
    pub approved_at: u64,
    #[serde(default = "png_type")]
    pub media_type: String,
    /// Scaled down to fit the 5 MiB limit before it was approved (round 40: "reduced to fit").
    #[serde(default)]
    pub reduced: bool,
    /// Claude has been sent this image. One approved while Claude waited and not yet sent goes
    /// with the next message, also after the report is closed and reopened.
    #[serde(default)]
    pub sent_to_claude: bool,
}

fn png_type() -> String {
    "image/png".into()
}

impl Store {
    /// The approved screenshot's bytes, checked against the hash it was approved with.
    pub fn shot_bytes(&self, draft_id: &str, shot: &Shot) -> Option<Vec<u8>> {
        let bytes = std::fs::read(self.draft_dir(draft_id).join("shots").join(&shot.id)).ok()?;
        (crate::util::sha256_hex(&bytes) == shot.sha256).then_some(bytes)
    }

    /// The manifest of a draft's approved screenshots, numbered 1..k in their order.
    pub fn manifest(&self, d: &Draft) -> Result<Vec<crate::image_profile::Entry>, String> {
        let entries: Result<Vec<_>, String> = d
            .screenshots
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let bytes = self.shot_bytes(&d.id, s).ok_or_else(|| format!("screenshot {} is missing on this Mac", i + 1))?;
                crate::image_profile::Entry::of(i as u32 + 1, &bytes)
            })
            .collect();
        let entries = entries?;
        crate::image_profile::check_set(&entries)?;
        Ok(entries)
    }
}

/// The approved report and where it stands. Approved is final: the draft's report and
/// screenshots are not changed once this exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delivery {
    pub approved: Approved,
    /// A state this version does not know (one saved by an earlier build) reads as not sent, so
    /// the report stays and can be sent again: never a lost report.
    #[serde(deserialize_with = "state_or_not_sent")]
    pub state: State,
}

fn state_or_not_sent<'de, D: serde::Deserializer<'de>>(d: D) -> Result<State, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(v).unwrap_or(State::NotSent { why: crate::delivery::NotSent::NoAnswer, at: 0 }))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub id: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub stage: Stage,
    pub description: String,
    /// The person declined diagnostic collection.
    #[serde(default)]
    pub no_diagnostics: bool,
    /// The person chose Prepare without AI, or Claude Code was unavailable.
    #[serde(default)]
    pub without_ai: bool,
    #[serde(default)]
    pub ai_failure: Option<crate::adapter::Failure>,
    #[serde(default)]
    pub conversation: Vec<Entry>,
    /// Questions asked, with the answer (`None`: not answered).
    #[serde(default)]
    pub questions: Vec<(String, Option<String>)>,
    /// The approximate time, as the person gave it and as the report says it.
    #[serde(default)]
    pub when: String,
    #[serde(default)]
    pub window: Option<crate::collect::Window>,
    /// The approved, covered screenshots, kept with the draft (owner-only, `shots/<id>.png`). Only
    /// the flattened copy the person approved is ever stored; the original never is.
    #[serde(default)]
    pub screenshots: Vec<Shot>,
    /// The report being reviewed (the current revision).
    #[serde(default)]
    pub report: Option<Report>,
    /// The approved report and its delivery.
    #[serde(default)]
    pub delivery: Option<Delivery>,
    /// How it was left: "draft, interrupted during the investigation".
    #[serde(default)]
    pub left: String,
}

impl Draft {
    pub fn new(now: u64) -> io::Result<Draft> {
        Ok(Draft {
            id: crate::util::new_id("d")?,
            created_at: now,
            updated_at: now,
            stage: Stage::Describe,
            description: String::new(),
            no_diagnostics: false,
            without_ai: false,
            ai_failure: None,
            conversation: Vec::new(),
            questions: Vec::new(),
            when: String::new(),
            window: None,
            screenshots: Vec::new(),
            report: None,
            delivery: None,
            left: String::new(),
        })
    }

    /// The title a list shows: the report's, or the first words of the description.
    pub fn title(&self) -> String {
        if let Some(d) = &self.delivery {
            return d.approved.title.clone();
        }
        if let Some(r) = &self.report {
            return r.title.clone();
        }
        let d = self.description.trim();
        if d.is_empty() { "Unfinished report".into() } else { d.chars().take(80).collect() }
    }
}

/// A save that did not happen, and why, in the words round 39 shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveError {
    pub disk_full: bool,
    pub message: String,
}

impl SaveError {
    fn of(e: &io::Error) -> SaveError {
        let disk_full = crate::util::is_disk_full(e);
        SaveError { disk_full, message: if disk_full { "the disk is full".into() } else { "its folder could not be written to".into() } }
    }
    /// The banner's sentence: "RichDoctor could not write to its folder: the disk is full."
    pub fn sentence(&self) -> String {
        if self.disk_full { "RichDoctor could not write to its folder: the disk is full.".into() } else { "RichDoctor could not write to its folder.".into() }
    }
}

/// What is left of a submitted report after its local copy expired.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sent {
    pub report_id: String,
    pub title: String,
    pub issue_url: String,
    pub sent_at: u64,
}

#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// The store under the account's home.
    pub fn for_home(home: &Path) -> Store {
        Store { root: home.join("Library/Application Support").join(crate::IDENTIFIER) }
    }
    pub fn at(root: &Path) -> Store {
        Store { root: root.to_path_buf() }
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn draft_dir(&self, id: &str) -> PathBuf {
        self.root.join("drafts").join(id)
    }
    /// The empty folder Claude Code runs in (never a user project).
    pub fn work_dir(&self) -> PathBuf {
        self.root.join("claude-work")
    }

    pub fn settings(&self) -> Settings {
        std::fs::read(self.root.join("settings.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Settings { start_at_login: true, ..Default::default() })
    }
    pub fn save_settings(&self, s: &Settings) -> Result<(), SaveError> {
        crate::util::write_private(&self.root.join("settings.json"), &serde_json::to_vec_pretty(s).expect("settings serialize")).map_err(|e| SaveError::of(&e))
    }
    /// The installation id: a random UUID, made once (the relay refuses any other shape). One
    /// saved in an earlier shape is replaced; it is a rate-limit key only, never an identity.
    pub fn installation_id(&self) -> String {
        let mut s = self.settings();
        if !crate::util::is_uuid(&s.installation_id) {
            s.installation_id = crate::util::uuid_v4().unwrap_or_default();
            self.save_settings(&s).ok();
        }
        s.installation_id
    }

    fn valid_id(id: &str) -> bool {
        id.len() == 14 && id.starts_with("d-") && id[2..].bytes().all(|b| b.is_ascii_hexdigit())
    }

    pub fn save(&self, draft: &mut Draft, now: u64) -> Result<(), SaveError> {
        if !Self::valid_id(&draft.id) {
            return Err(SaveError { disk_full: false, message: "not a draft id".into() });
        }
        draft.updated_at = now;
        let bytes = serde_json::to_vec_pretty(draft).expect("draft serializes");
        crate::util::write_private(&self.draft_dir(&draft.id).join("draft.json"), &bytes).map_err(|e| SaveError::of(&e))
    }

    pub fn load(&self, id: &str) -> Option<Draft> {
        if !Self::valid_id(id) {
            return None;
        }
        serde_json::from_slice(&std::fs::read(self.draft_dir(id).join("draft.json")).ok()?).ok()
    }

    /// Every saved draft, newest first.
    pub fn list(&self) -> Vec<Draft> {
        let Ok(entries) = std::fs::read_dir(self.root.join("drafts")) else { return Vec::new() };
        let mut out: Vec<Draft> = entries.filter_map(|e| e.ok()).filter_map(|e| self.load(e.file_name().to_str()?)).collect();
        out.sort_by_key(|d| std::cmp::Reverse(d.updated_at));
        out
    }

    /// Reports whose local copy expired: only their links.
    pub fn sent(&self) -> Vec<Sent> {
        let Ok(entries) = std::fs::read_dir(self.root.join("sent")) else { return Vec::new() };
        let mut out: Vec<Sent> = entries.filter_map(|e| e.ok()).filter_map(|e| serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok()).collect();
        out.sort_by_key(|s| std::cmp::Reverse(s.sent_at));
        out
    }

    /// **DELETE LOCAL COPY.** The draft's folder, its evidence with it. Nothing public changes.
    pub fn delete(&self, id: &str) -> io::Result<()> {
        if !Self::valid_id(id) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a draft id"));
        }
        std::fs::remove_dir_all(self.draft_dir(id))
    }

    /// Remove submitted reports' local copies 30 days after sending, keeping each link.
    pub fn expire(&self, now: u64) -> usize {
        let mut n = 0;
        for d in self.list() {
            if let Some(Delivery { approved, state: State::Submitted { issue_url, at, .. }, .. }) = &d.delivery {
                if now.saturating_sub(*at) >= SUBMITTED_KEPT_MS {
                    let sent = Sent { report_id: approved.report_id.clone(), title: approved.title.clone(), issue_url: issue_url.clone(), sent_at: *at };
                    if crate::util::write_private(&self.root.join("sent").join(format!("{}.json", approved.report_id)), &serde_json::to_vec(&sent).expect("serializes")).is_ok() && self.delete(&d.id).is_ok() {
                        n += 1;
                    }
                }
            }
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (PathBuf, Store) {
        let root = std::env::temp_dir().join(format!("rd-store-{}", crate::util::hex(&crate::util::random_bytes(4).unwrap())));
        (root.clone(), Store::at(&root))
    }

    /// The relay's own pattern (checks.mjs `UUID`): 8-4-4-4-12 hex digits.
    fn relay_uuid(s: &str) -> bool {
        let parts: Vec<&str> = s.split('-').collect();
        parts.iter().map(|p| p.len()).collect::<Vec<_>>() == [8, 4, 4, 4, 12] && parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_hexdigit()))
    }

    #[test]
    fn the_installation_id_is_the_uuid_the_relay_requires_and_stays_the_same() {
        let (root, s) = store();
        let id = s.installation_id();
        assert!(relay_uuid(&id), "{id}");
        assert_eq!(s.installation_id(), id, "made once");
        // An installation that saved the earlier format gets a valid one.
        let mut old = s.settings();
        old.installation_id = "inst-173fb086d09f".into();
        s.save_settings(&old).unwrap();
        let fresh = s.installation_id();
        assert!(relay_uuid(&fresh) && fresh != "inst-173fb086d09f", "{fresh}");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn a_draft_survives_quit_and_restart_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let (root, s) = store();
        let mut d = Draft::new(1).unwrap();
        d.description = "RichOS closes before its window opens.".into();
        d.conversation.push(Entry::Question { text: "When?".into(), choices: vec!["Just now".into()] });
        d.questions.push(("When?".into(), None));
        s.save(&mut d, 5).unwrap();
        let back = s.load(&d.id).unwrap();
        assert_eq!(back, d);
        let mode = std::fs::metadata(s.draft_dir(&d.id).join("draft.json")).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(s.list().len(), 1);
        s.delete(&d.id).unwrap();
        assert!(s.list().is_empty());
        assert!(s.load("../settings").is_none());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn an_unwritable_folder_is_reported_not_hidden() {
        use std::os::unix::fs::PermissionsExt;
        let (root, s) = store();
        std::fs::create_dir_all(root.join("drafts")).unwrap();
        std::fs::set_permissions(root.join("drafts"), std::fs::Permissions::from_mode(0o500)).unwrap();
        let mut d = Draft::new(1).unwrap();
        let e = s.save(&mut d, 2).unwrap_err();
        assert!(!e.disk_full);
        assert_eq!(e.sentence(), "RichDoctor could not write to its folder.");
        std::fs::set_permissions(root.join("drafts"), std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn a_report_saved_by_an_earlier_build_is_kept_and_can_be_sent_again() {
        let (root, s) = store();
        let mut d = Draft::new(1).unwrap();
        let body = crate::report::footer("RD-7K3M-Q9VX");
        let approved = crate::delivery::approve("RD-7K3M-Q9VX", "t", &body, vec![], "https://report.richos.ceo", 1, "0b6f3c2e-5d41-4a8e-9f27-3c1d2e4f5a6b").unwrap();
        d.delivery = Some(Delivery { approved, state: State::Sending });
        s.save(&mut d, 2).unwrap();
        // The earlier build's record: a state this one does not have, and its extra fields.
        let file = s.draft_dir(&d.id).join("draft.json");
        let mut v: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        v["delivery"]["state"] = serde_json::json!({"state": "unconfirmed", "checked_at": 2});
        v["delivery"]["completion_attempted"] = serde_json::json!(true);
        std::fs::write(&file, serde_json::to_vec(&v).unwrap()).unwrap();
        let back = s.load(&d.id).expect("the report is kept");
        assert!(back.delivery.unwrap().state.may_send());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn a_submitted_copy_expires_after_30_days_and_its_link_stays() {
        let (root, s) = store();
        let mut d = Draft::new(1).unwrap();
        let body = crate::report::footer("RD-7K3M-Q9VX");
        let approved = crate::delivery::approve("RD-7K3M-Q9VX", "Title", &body, vec![], "https://report.richos.ceo", 1, "0b6f3c2e-5d41-4a8e-9f27-3c1d2e4f5a6b").unwrap();
        d.delivery = Some(Delivery { approved, state: State::Submitted { issue_url: "https://github.com/a/b/issues/1".into(), issue_number: 1, at: 10 } });
        s.save(&mut d, 10).unwrap();
        assert_eq!(s.expire(10 + SUBMITTED_KEPT_MS - 1), 0);
        assert_eq!(s.expire(10 + SUBMITTED_KEPT_MS), 1);
        assert!(s.list().is_empty());
        assert_eq!(s.sent()[0].issue_url, "https://github.com/a/b/issues/1");
        std::fs::remove_dir_all(root).ok();
    }
}
