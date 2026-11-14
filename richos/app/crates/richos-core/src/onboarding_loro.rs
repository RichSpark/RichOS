//! WHAT ONBOARDING LEARNS, ADDED TO LORO BY ITSELF.
//!
//! The CEO, 2026-11-14: *"whatever info gets added during onboarding, should be automatically
//! added to loro. I hope that's already happening that way. But if the user hasn't yet provided
//! any info for onboarding or if the background work for onboarding hasn't finished yet, then the
//! current default is right."*
//!
//! It was not happening. Onboarding keeps what it learns in the central folder: the user's
//! answers and Rich's notes in `companies/<id>/company.md` (`onboarding_tools.rs`
//! `save_company_notes`), the user's own details in `person.md` (`save_person`), and the team's
//! research beside the company notes (Stu's dossier, `engine/app/agents/stu.md`). None of it
//! reached the loro corpus, which is what the home screen and Rich's company memory read.
//!
//! So this copies it there. Every Markdown file directly in a company's notes folder becomes one
//! record in that company's loro partition, and `person.md` one record in the `ceo` partition. No
//! file name is special: whatever the onboarding saves as Markdown beside the company notes is
//! taken, so the instruction files can name their files as they like (§124). A file that changes
//! supersedes its previous record, so loro keeps one current record per file and the history
//! behind it. Nothing is ever deleted from loro. With nothing saved yet there is nothing to copy
//! and nothing changes, which is the "current default" of his sentence.
//!
//! This is not the inference §7 guards against ("Nothing is ever learned silently",
//! `correction.rs`): these files are what the user told Rich and what the team found at the
//! user's request during onboarding, already saved by onboarding. The CEO's sentence above is
//! the ruling that they go to loro without a second ask.
//!
//! The writer is loro's own (`loro-write.mjs`, through [`CliLoroWriter`]), never a file written
//! into the corpus by hand. What is already copied is kept in a small file in the app's data
//! folder, so a turn that changed nothing starts no process at all.

use crate::correction::CliLoroWriter;
use crate::entity::{EntityId, EntityRegistry};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The kept file, in the app's data folder.
pub const KEPT_FILENAME: &str = "onboarding-loro.json";

/// The kind every copied record is written as. A file of notes is a set of facts about the user
/// or the company; the writer requires a declared kind.
pub const RECORD_KIND: &str = "fact";

/// One file to copy into loro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// Stable for the file: `person`, or `company/<id>/<file name>`.
    pub key: String,
    /// The company the record belongs to (id, display name), or `None` for the `ceo` partition.
    pub company: Option<(String, String)>,
    /// The record id's readable part, before the content hash.
    pub stem: String,
    pub title: String,
    pub body: String,
    /// The file it came from, written into the record's provenance.
    pub path: PathBuf,
}

impl Source {
    /// `ceo`, or the company id: the writer's `--partition`.
    pub fn partition(&self) -> &str {
        self.company.as_ref().map(|(id, _)| id.as_str()).unwrap_or("ceo")
    }

    /// The first eight hex digits of the SHA-256 of what the record says.
    pub fn hash(&self) -> String {
        use sha2::Digest;
        let digest = sha2::Sha256::digest(format!("{}\n{}\n{}", self.partition(), self.title, self.body).as_bytes());
        format!("{digest:x}")[..8].to_string()
    }

    /// The record id: the stem, made a legal loro id (`[a-z0-9][a-z0-9-]{0,79}`), and the hash.
    /// A new version of the file is a new id, so it can supersede the old one.
    pub fn record_id(&self) -> String {
        let mut slug = String::new();
        for c in self.stem.chars() {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                slug.push(c);
            } else if !slug.ends_with('-') && !slug.is_empty() {
                slug.push('-');
            }
        }
        slug.truncate(70);
        let slug = slug.trim_end_matches('-');
        let slug = if slug.is_empty() { "onboarding" } else { slug };
        format!("{slug}-{}", self.hash())
    }
}

/// The interview marker `company::save_company_notes` puts on the first line. It is the app's
/// bookkeeping, not something the user said.
fn without_marker(text: &str) -> &str {
    let text = text.trim();
    for marker in [crate::company::PARTIAL_MARKER, crate::company::COMPLETE_MARKER] {
        if let Some(rest) = text.strip_prefix(marker) {
            return rest.trim();
        }
    }
    text
}

/// What onboarding has saved so far, in a stable order: the user's own details, then each
/// company's Markdown files by name. A file that is missing, empty or unreadable is not a source.
pub fn sources(central_root: &Path, registry: &EntityRegistry) -> Vec<Source> {
    let mut out = Vec::new();
    let names: Vec<&str> = registry.entities().iter().map(|e| e.display_name.trim()).filter(|n| !n.is_empty()).collect();
    if let Some(details) = crate::person::read(central_root) {
        // Which companies the details belong with, from the app's own list, so the record is
        // found when the user's companies are asked about.
        let body = if names.is_empty() { details } else { format!("{details}\n\nTheir companies in RichOS: {}.", names.join(", ")) };
        out.push(Source {
            key: "person".into(),
            company: None,
            stem: "person".into(),
            title: "The user's own details".into(),
            body,
            path: crate::person::path(central_root),
        });
    }
    for entity in registry.entities() {
        let Ok(id) = EntityId::parse(entity.id.as_str()) else { continue };
        let dir = crate::company::company_home(central_root, &id);
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        let mut files: Vec<PathBuf> = read
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .filter(|p| p.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("md")))
            .filter(|p| !p.file_name().and_then(|n| n.to_str()).unwrap_or(".").starts_with('.'))
            .collect();
        files.sort();
        let name = entity.display_name.trim();
        let name = if name.is_empty() { id.as_str() } else { name };
        for path in files {
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let body = without_marker(&text);
            if body.is_empty() {
                continue;
            }
            let file = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            let stem = path.file_stem().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            let what = if file.eq_ignore_ascii_case(crate::company::COMPANY_FILENAME) {
                "company notes".to_string()
            } else {
                stem.replace(['-', '_'], " ")
            };
            out.push(Source {
                key: format!("company/{}/{file}", id.as_str()),
                company: Some((id.as_str().to_string(), name.to_string())),
                stem: format!("{}-{stem}", id.as_str()),
                title: format!("{name}: {what}"),
                body: body.to_string(),
                path,
            });
        }
    }
    out
}

/// The three writes the copy needs, behind a trait so the decisions are tested without node.
pub trait OnboardingLoroWriter {
    /// A new record; returns its ref.
    fn append(&self, source: &Source, id: &str) -> Result<String, String>;
    /// A new record that replaces `old`; returns the new ref.
    fn supersede(&self, source: &Source, old: &str, id: &str) -> Result<String, String>;
    /// The company's partition, for a company added after the memory folder was made.
    fn create_company(&self, id: &str, name: &str) -> Result<(), String>;
}

fn record_args(source: &Source) -> Vec<String> {
    vec![
        "--kind".into(),
        RECORD_KIND.into(),
        "--title".into(),
        source.title.clone(),
        "--tags".into(),
        "onboarding".into(),
        "--source".into(),
        "onboarding".into(),
        "--method".into(),
        "onboarding".into(),
        "--ref-source".into(),
        source.path.display().to_string(),
        "--body-stdin".into(),
    ]
}

impl OnboardingLoroWriter for CliLoroWriter {
    fn append(&self, source: &Source, id: &str) -> Result<String, String> {
        let mut args = vec!["--partition".into(), source.partition().to_string(), "--id".into(), id.to_string()];
        args.extend(record_args(source));
        match self.run_verb("append", &args, Some(&source.body)) {
            Ok(out) => Ok(out.r#ref),
            Err(e) => {
                // The same file and the same words were written before and the kept file lost
                // it: the writer names the record that holds them, and that is the one to keep.
                let why = e.to_string();
                match why.split('"').nth(1).filter(|r| r.starts_with("rec:") && why.contains("already exists")) {
                    Some(existing) if existing.ends_with(&format!("/{id}")) => Ok(existing.to_string()),
                    _ => Err(why),
                }
            }
        }
    }

    fn supersede(&self, source: &Source, old: &str, id: &str) -> Result<String, String> {
        let mut args = vec!["--ref".into(), old.to_string(), "--id".into(), id.to_string(), "--why".into(),
            "Onboarding saved a newer version of this.".into()];
        args.extend(record_args(source));
        self.run_verb("supersede", &args, Some(&source.body)).map(|out| out.r#ref).map_err(|e| e.to_string())
    }

    fn create_company(&self, id: &str, name: &str) -> Result<(), String> {
        let args = vec!["--id".into(), id.to_string(), "--name".into(), name.to_string()];
        self.run_verb("create-company", &args, None).map(|_| ()).map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Kept {
    /// The corpus these records are in. A different corpus starts again.
    corpus: String,
    records: BTreeMap<String, KeptRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct KeptRecord {
    hash: String,
    #[serde(rename = "ref")]
    record: String,
}

fn load(path: &Path) -> Kept {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(path: &Path, kept: &Kept) -> Result<(), String> {
    let text = serde_json::to_string_pretty(kept).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// What one copy did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncReport {
    /// `<key> -> <ref>` for each record written.
    pub written: Vec<String>,
    /// `<key>: <the writer's own words>` for each file that could not be copied. It is tried
    /// again at the next copy.
    pub problems: Vec<String>,
}

/// One copy at a time in this process: the end of a turn and the boot can come together.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn append_into(writer: &dyn OnboardingLoroWriter, source: &Source, id: &str) -> Result<String, String> {
    match writer.append(source, id) {
        Ok(r) => Ok(r),
        // A company added after the memory folder was made has no partition yet. Make it, the
        // way memory setup made the others (`provision.rs`), and write once more.
        Err(first) => match &source.company {
            Some((company, name)) => {
                let made = writer.create_company(company, name);
                writer.append(source, id).map_err(|again| match made {
                    Ok(()) => again,
                    Err(why) => format!("{first}; making the company's place in loro also failed: {why}"),
                })
            }
            None => Err(first),
        },
    }
}

/// Copy every source that is new or changed since the last copy into loro.
pub fn sync(sources: &[Source], writer: &dyn OnboardingLoroWriter, corpus: &str, kept_path: &Path) -> SyncReport {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|p| p.into_inner());
    let mut kept = load(kept_path);
    if kept.corpus != corpus {
        kept = Kept { corpus: corpus.to_string(), records: BTreeMap::new() };
    }
    let mut report = SyncReport::default();
    for source in sources {
        let hash = source.hash();
        let previous = kept.records.get(&source.key).cloned();
        if previous.as_ref().is_some_and(|p| p.hash == hash) {
            continue;
        }
        let id = source.record_id();
        let written = match &previous {
            // A record that cannot be superseded (moved or removed by hand) is no reason to
            // lose the new version: it is written as a new record.
            Some(p) => writer.supersede(source, &p.record, &id).or_else(|_| append_into(writer, source, &id)),
            None => append_into(writer, source, &id),
        };
        match written {
            Ok(record) => {
                report.written.push(format!("{} -> {record}", source.key));
                kept.records.insert(source.key.clone(), KeptRecord { hash, record });
                if let Err(e) = save(kept_path, &kept) {
                    report.problems.push(format!("{}: written, but {} could not be saved ({e})", source.key, kept_path.display()));
                }
            }
            Err(why) => report.problems.push(format!("{}: {why}", source.key)),
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Entity, EntityRegistry};
    use std::cell::RefCell;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("richos-onboarding-loro-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn registry(companies: &[(&str, &str)]) -> EntityRegistry {
        let mut r = EntityRegistry::default();
        for (id, name) in companies {
            r.register(Entity::try_new(id, name, Vec::new()).unwrap()).unwrap();
        }
        r
    }

    #[derive(Default)]
    struct Fake {
        calls: RefCell<Vec<String>>,
        partitions: RefCell<Vec<String>>,
    }

    impl OnboardingLoroWriter for Fake {
        fn append(&self, s: &Source, id: &str) -> Result<String, String> {
            self.calls.borrow_mut().push(format!("append {} {id}", s.partition()));
            if s.partition() != "ceo" && !self.partitions.borrow().iter().any(|p| p == s.partition()) {
                return Err(format!("loro write: no company \"{}\" in this corpus.", s.partition()));
            }
            Ok(format!("rec:{}/records/{id}", s.partition()))
        }
        fn supersede(&self, s: &Source, old: &str, id: &str) -> Result<String, String> {
            self.calls.borrow_mut().push(format!("supersede {old} {id}"));
            Ok(format!("rec:{}/records/{id}", s.partition()))
        }
        fn create_company(&self, id: &str, _name: &str) -> Result<(), String> {
            self.calls.borrow_mut().push(format!("create {id}"));
            self.partitions.borrow_mut().push(id.into());
            Ok(())
        }
    }

    #[test]
    fn nothing_saved_yet_is_nothing_to_copy() {
        let root = tmp();
        assert!(sources(&root, &registry(&[("harborline", "Harborline")])).is_empty());
    }

    #[test]
    fn the_answers_the_research_and_the_person_are_each_a_source() {
        let root = tmp();
        let id = EntityId::parse("harborline").unwrap();
        crate::company::save_company_notes(&root, &id, "Sells ferry tickets.", crate::company::InterviewProgress::Partial).unwrap();
        std::fs::write(crate::company::company_home(&root, &id).join("onboarding-research.md"), "Founded 2019.\n").unwrap();
        std::fs::write(crate::company::company_home(&root, &id).join("first-piece.html"), "<p>not markdown</p>").unwrap();
        crate::person::save(&root, "Dana Reyes, call her Dana.").unwrap();
        let got = sources(&root, &registry(&[("harborline", "Harborline")]));
        let keys: Vec<_> = got.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(keys, ["person", "company/harborline/company.md", "company/harborline/onboarding-research.md"]);
        assert_eq!(got[1].body, "Sells ferry tickets.", "the interview marker is the app's, not the user's");
        assert_eq!(got[1].title, "Harborline: company notes");
        assert_eq!(got[2].title, "Harborline: onboarding research");
        assert!(got[0].body.ends_with("Their companies in RichOS: Harborline."));
        for s in &got {
            let id = s.record_id();
            assert!(id.len() <= 80 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'), "{id}");
        }
    }

    #[test]
    fn a_copy_writes_once_makes_the_partition_and_supersedes_a_change() {
        let root = tmp();
        let kept = root.join(KEPT_FILENAME);
        let id = EntityId::parse("harborline").unwrap();
        let reg = registry(&[("harborline", "Harborline")]);
        crate::company::save_company_notes(&root, &id, "Sells ferry tickets.", crate::company::InterviewProgress::Partial).unwrap();
        let fake = Fake::default();

        let first = sync(&sources(&root, &reg), &fake, "/corpus", &kept);
        assert!(first.problems.is_empty(), "{:?}", first.problems);
        assert_eq!(first.written.len(), 1);
        let calls = fake.calls.borrow().clone();
        assert_eq!(calls.len(), 3, "append refused, partition made, append again: {calls:?}");
        assert!(calls[1] == "create harborline");

        fake.calls.borrow_mut().clear();
        let again = sync(&sources(&root, &reg), &fake, "/corpus", &kept);
        assert!(again.written.is_empty() && fake.calls.borrow().is_empty(), "an unchanged file starts no writer");

        crate::company::save_company_notes(&root, &id, "Sells ferry tickets and cargo space.", crate::company::InterviewProgress::Complete).unwrap();
        let changed = sync(&sources(&root, &reg), &fake, "/corpus", &kept);
        assert_eq!(changed.written.len(), 1);
        assert!(fake.calls.borrow()[0].starts_with("supersede rec:harborline/records/harborline-company-"), "{:?}", fake.calls.borrow());

        fake.calls.borrow_mut().clear();
        sync(&sources(&root, &reg), &fake, "/another-corpus", &kept);
        assert!(fake.calls.borrow()[0].starts_with("append "), "a different corpus gets its own records");
    }
}
