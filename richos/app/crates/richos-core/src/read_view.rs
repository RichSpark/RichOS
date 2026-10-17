//! Published conversation state for readers that must not wait for a compute turn.
//!
//! The writer folds each successfully appended event into the same ledger projection used
//! by replay. Readers retain an immutable version. Copy-on-write copies history only when
//! a reader still holds an older version; it never opens another writer or replays disk.

use crate::{entity::{EntityRegistry, ThreadBinding}, journal::MachineryJournal,
    ledger::{Event, Ledger, Message}, onboarding::OnboardingState,
    spine::{Spine, SpineError, WorkerEventsSource}, steering::TurnControl,
    thread::{summaries, ThreadSummary}, timeline::Timeline};
use std::{path::PathBuf, sync::{Arc, RwLock}};

/// Read capabilities shared by the live spine and its published view.
pub trait SpineView {
    fn ledger(&self) -> &Ledger;
    fn entity_registry(&self) -> &EntityRegistry;
    fn active_binding(&self) -> Option<&ThreadBinding>;
    fn timeline(&self, thread: &str) -> Result<Timeline, SpineError>;
    fn machinery_journal(&self) -> Option<&MachineryJournal>;
    fn onboarding_state(&self, binding: &ThreadBinding) -> OnboardingState;
    fn threads(&self) -> Vec<ThreadSummary> { summaries(self.ledger()) }
    fn messages(&self, thread: &str) -> Result<Vec<Message>, SpineError> {
        Ok(self.ledger().messages(thread)?)
    }
    fn active_thread(&self) -> Option<&str> { self.active_binding().map(|b| b.thread_id()) }
}

impl SpineView for Spine {
    fn ledger(&self) -> &Ledger { self.ledger() }
    fn entity_registry(&self) -> &EntityRegistry { self.entity_registry() }
    fn active_binding(&self) -> Option<&ThreadBinding> { self.active_binding() }
    fn timeline(&self, thread: &str) -> Result<Timeline, SpineError> { self.timeline(thread) }
    fn machinery_journal(&self) -> Option<&MachineryJournal> { self.machinery_journal() }
    fn onboarding_state(&self, binding: &ThreadBinding) -> OnboardingState { self.onboarding_state(binding) }
}

#[derive(Clone)]
pub(crate) struct ReadMetadata {
    pub active: Option<ThreadBinding>,
    pub registry: EntityRegistry,
    pub machinery_root: Option<PathBuf>,
    pub workers: WorkerEventsSource,
    pub control: TurnControl,
    pub central_root: Option<PathBuf>,
    pub onboarding_record: Option<PathBuf>,
}

pub struct ReadView {
    ledger: Ledger,
    metadata: ReadMetadata,
    journal: Option<MachineryJournal>,
}

impl Clone for ReadView {
    fn clone(&self) -> Self {
        Self { ledger: self.ledger.read_copy(), metadata: self.metadata.clone(),
            journal: self.metadata.machinery_root.as_ref().map(MachineryJournal::new) }
    }
}

impl SpineView for ReadView {
    fn ledger(&self) -> &Ledger { &self.ledger }
    fn entity_registry(&self) -> &EntityRegistry { &self.metadata.registry }
    fn active_binding(&self) -> Option<&ThreadBinding> { self.metadata.active.as_ref() }
    fn machinery_journal(&self) -> Option<&MachineryJournal> { self.journal.as_ref() }
    fn timeline(&self, thread: &str) -> Result<Timeline, SpineError> {
        let binding = self.ledger.thread_binding(thread)?;
        let machinery = self.journal.as_ref().map(|j| j.read_thread(thread)).unwrap_or_default();
        let workers = self.metadata.workers.read(self.metadata.control.for_thread(thread).lease_session().as_deref());
        Ok(Timeline::project_with_workers(&self.ledger, &binding, &machinery, &workers)?)
    }
    fn onboarding_state(&self, binding: &ThreadBinding) -> OnboardingState {
        let layer = self.metadata.central_root.as_ref()
            .map(|root| crate::company::CompanyLayer::read(root, binding.entity_id()));
        let record = self.metadata.onboarding_record.as_ref()
            .map(|path| crate::onboarding::OnboardingRecord::load(path).for_entity(binding.entity_id()))
            .unwrap_or_default();
        crate::onboarding::state(layer.as_ref(), &record)
    }
}

impl ReadView {
    /// **HE SAID "NOT NOW", written down without the spine.** The same write as
    /// [`Spine::record_onboarding_declination`], from the published view.
    ///
    /// The write needs two facts, where the record lives and which company is open, and both
    /// are published here. Taking the spine for them put "Not now" behind whatever held it: at
    /// boot that is `prime_front_desk`, a model turn, and on 2026-10-11 the card stayed on
    /// screen 90899 ms. Nothing else is needed: before every message each desk compares the
    /// block it was primed with against this record and re-primes if they differ
    /// (`Spine::prime_lease_if_needed`), so the next message on any desk carries the decline.
    ///
    /// The same refusals as the spine's: no record configured, or no company open.
    pub fn record_onboarding_declination(&self, now_millis: u64) -> Result<(), crate::doctrine::DoctrineError> {
        let path = self.metadata.onboarding_record.as_ref().ok_or_else(|| crate::doctrine::DoctrineError::Unwritable {
            path: "<no onboarding record configured>".into(),
            why: "nobody told this spine where the declination record lives, so there is nowhere to write".into(),
        })?;
        let binding = self.metadata.active.as_ref().ok_or_else(|| crate::doctrine::DoctrineError::Unwritable {
            path: path.display().to_string(), why: "no company is selected; no answer was recorded".into(),
        })?;
        crate::onboarding::OnboardingRecord::record_declination_for_entity(path, binding.entity_id(), now_millis)
    }
}

#[derive(Clone)]
pub struct SpineReader(Arc<RwLock<Arc<ReadView>>>);

impl SpineReader {
    pub(crate) fn new(spine: &Spine) -> Self {
        let metadata = spine.read_metadata();
        let journal = metadata.machinery_root.as_ref().map(MachineryJournal::new);
        Self(Arc::new(RwLock::new(Arc::new(ReadView {
            ledger: spine.ledger().read_copy(), metadata, journal,
        }))))
    }

    /// The lock protects only publication. Projection, serialization and file reads happen
    /// after it is released, so a slow window cannot hold up the writer's next event.
    pub fn snapshot(&self) -> Arc<ReadView> { self.0.read().unwrap().clone() }

    pub(crate) fn apply(&self, event: Event) {
        Arc::make_mut(&mut self.0.write().unwrap()).ledger.apply_read_event(event);
    }

    pub(crate) fn refresh(&self, spine: &Spine) {
        let mut published = self.0.write().unwrap();
        let view = Arc::make_mut(&mut published);
        view.metadata = spine.read_metadata();
        view.journal = view.metadata.machinery_root.as_ref().map(MachineryJournal::new);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cognition::MockCognition, entity::{Entity, EntityId}, ledger::Source};

    #[test]
    fn published_versions_stay_consistent_across_writes_and_switches() {
        let path = std::env::temp_dir().join(format!("read-view-{}.jsonl", std::process::id()));
        if let Err(error) = std::fs::remove_file(&path) {
            assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        }
        let mut spine = Spine::new(Ledger::open(&path).unwrap());
        let reader = spine.reader();
        let empty = reader.snapshot();
        spine.set_entity_registry(EntityRegistry::new(vec![
            Entity::new("alpha", "Alpha", &["/fixture/alpha"]).unwrap(),
            Entity::new("beta", "Beta", &["/fixture/beta"]).unwrap(),
        ]).unwrap());
        let a = spine.create_thread("First", &EntityId::parse("alpha").unwrap()).unwrap();
        let first = reader.snapshot();
        let b = spine.create_thread("Second", &EntityId::parse("beta").unwrap()).unwrap();
        spine.switch_thread(&b).unwrap();
        spine.attach_lease(Box::new(MockCognition::new("snapshot-test", vec!["Only in Beta"])));
        spine.submit_prompt("Beta question", Source::Text).unwrap();
        let second = reader.snapshot();
        assert!(empty.threads().is_empty());
        assert_eq!(first.threads().len(), 1);
        assert_eq!(first.active_thread(), Some(a.as_str()));
        assert!(first.messages(&b).is_err());
        assert_eq!(second.active_binding(), spine.active_binding());
        assert!(second.active_binding().unwrap().binding_revision() > first.active_binding().unwrap().binding_revision());
        assert!(second.messages(&a).unwrap().is_empty());
        assert_eq!(second.messages(&b).unwrap(), spine.messages(&b).unwrap());
        assert_eq!(second.timeline(&b).unwrap().view(crate::timeline::ViewMode::Ceo).payload(),
            spine.timeline(&b).unwrap().view(crate::timeline::ViewMode::Ceo).payload());
        assert_eq!(first.messages(&a).unwrap().len(), 0);
        assert!(second.messages("missing").is_err());
        drop(spine);
        std::fs::remove_file(path).unwrap();
    }
}
