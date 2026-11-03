//! Hunt v5 G7: recovery after interrupted writes.
#[cfg(test)]
use crate::bug_report as bug;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{fs, path::PathBuf};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("local-records-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) { fs::remove_dir_all(&self.0).unwrap(); }
}
fn public() -> bug::Public {
    let plain = |s: &str| vec![bug::Segment { text: s.into(), was: None, kind: None }];
    let draft = bug::Draft { title: plain("Window froze"), sections: vec![bug::DraftSection {
        heading: "What happened".into(), paragraphs: vec![plain("It froze.")], steps: vec![],
    }], private: vec![] };
    match bug::at_send(&bug::sheet_of(&draft), Some(&bug::Check::of(&draft)), &[], |_| panic!("already checked")) {
        bug::AtSend::Send(p) => p, _ => panic!("not approved"),
    }
}
struct Cred;
impl bug::Credentials for Cred {
    fn credential(&self) -> Option<bug::Credential> { Some(bug::Credential { account: bug::Account::Reporting, token: String::new() }) }
}
struct CleanupFailure { dir: PathBuf, calls: AtomicUsize }
impl bug::Transport for CleanupFailure {
    fn create_issue(&self, _: &bug::Credential, _: &str, _: &serde_json::Value) -> bug::Outcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        // Keep the exact pending bytes readable to read_all, but obstruct remove_file.
        let p = fs::read_dir(self.dir.join("waiting")).unwrap().next().unwrap().unwrap().path();
        let bytes = fs::read(&p).unwrap();
        fs::remove_file(&p).unwrap();
        fs::create_dir(&p).unwrap();
        fs::write(self.dir.join("waiting/copy.json"), bytes).unwrap();
        bug::Outcome::Created { number: 42, url: "https://fixture.invalid/issues/42".into() }
    }
    fn graphql(&self, _: &bug::Credential, _: &serde_json::Value) -> bug::Reply { panic!("no network") }
}
#[test]
fn p1_54_sent_receipt_wins_over_failed_cleanup() {
    let scratch = Scratch::new();
    let outbox = bug::Outbox::open(&scratch.0);
    let transport = CleanupFailure { dir: scratch.0.clone(), calls: AtomicUsize::new(0) };
    let first = outbox.send(&public(), &Cred, &transport, 1);
    let sent = outbox.sent().unwrap().pop().unwrap();
    assert!(matches!(first, Ok(bug::Delivery::Sent(_))), "delivery succeeded despite cleanup: {first:?}");
    assert!(matches!(outbox.gone(&sent.id).unwrap(), Some(bug::Withdrawn::AlreadySent(_))));
    assert!(matches!(outbox.withdraw(&sent.id).unwrap(), bug::Withdrawn::AlreadySent(_)));
    assert!(outbox.pending().unwrap().is_empty());
    assert!(outbox.try_now(&sent.id, &Cred, &transport, u64::MAX).unwrap().is_none());
    assert!(outbox.send_due(&Cred, &transport, u64::MAX).unwrap().is_empty());
    assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
    assert_eq!(outbox.sent().unwrap().len(), 1);
}
