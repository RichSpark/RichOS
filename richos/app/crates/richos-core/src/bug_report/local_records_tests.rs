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

struct RelayMock {
    dir: PathBuf,
    identities: std::cell::RefCell<Vec<bug::RelayIdentity>>,
    prepared: AtomicUsize,
    obstruct: bool,
}
impl bug::Transport for RelayMock {
    fn prepare_report(&self) -> std::io::Result<Option<bug::RelayIdentity>> {
        self.prepared.fetch_add(1, Ordering::SeqCst);
        if self.obstruct {
            let path = fs::read_dir(self.dir.join("waiting"))?.next().unwrap()?.path();
            fs::remove_file(&path)?;
            fs::create_dir(path)?;
        }
        Ok(Some(bug::RelayIdentity { id: "RD-ABCD-EFGH".into(), receipt_secret: "fixture-only-key".into() }))
    }
    fn post_report(&self, _: &bug::Credential, _: bug::Post, pending: &bug::Pending) -> bug::Outcome {
        let saved: bug::Pending = serde_json::from_slice(&fs::read(self.dir.join("waiting").join(format!("{}.json", pending.id))).unwrap()).unwrap();
        assert_eq!(saved.relay, pending.relay, "identity must be on disk before the network call");
        let mut identities = self.identities.borrow_mut();
        identities.push(pending.relay.clone().unwrap());
        if identities.len() == 1 { bug::Outcome::TimedOut }
        else { bug::Outcome::Created { number: 42, url: "https://fixture.invalid/issues/42".into() } }
    }
    fn create_issue(&self, _: &bug::Credential, _: &str, _: &serde_json::Value) -> bug::Outcome { panic!("relay identity must be passed to post_report") }
    fn graphql(&self, _: &bug::Credential, _: &serde_json::Value) -> bug::Reply { panic!("no network") }
}
#[test]
fn p1_57_relay_identity_is_persisted_before_sending_and_reused_after_reopen() {
    let s = Scratch::new();
    let relay = RelayMock { dir: s.0.clone(), identities: Default::default(), prepared: AtomicUsize::new(0), obstruct: false };
    let outbox = bug::Outbox::open(&s.0);
    let bug::Delivery::Waiting { id, .. } = outbox.send(&public(), &Cred, &relay, 1).unwrap() else { panic!("answer must be lost") };
    drop(outbox);
    let reopened = bug::Outbox::open(&s.0);
    assert!(matches!(reopened.try_now(&id, &Cred, &relay, 2).unwrap(), Some(bug::Delivery::Sent(_))));
    let identities = relay.identities.borrow();
    assert_eq!(identities.len(), 2);
    assert_eq!(identities[0], identities[1]);
    assert_eq!(relay.prepared.load(Ordering::SeqCst), 1);
}
#[test]
fn p1_57_an_identity_that_cannot_be_saved_sends_nothing() {
    let s = Scratch::new();
    let relay = RelayMock { dir: s.0.clone(), identities: Default::default(), prepared: AtomicUsize::new(0), obstruct: true };
    assert!(bug::Outbox::open(&s.0).send(&public(), &Cred, &relay, 1).is_err());
    assert!(relay.identities.borrow().is_empty());
}

struct ReceiptFixture(AtomicUsize);
impl bug::Transport for ReceiptFixture {
    fn create_issue(&self, _: &bug::Credential, _: &str, _: &serde_json::Value) -> bug::Outcome {
        let n = self.0.fetch_add(1, Ordering::SeqCst) + 1;
        if n == 2 || n == 4 { bug::Outcome::Unreachable }
        else { bug::Outcome::Created { number: n as u64, url: format!("https://fixture.invalid/issues/{n}") } }
    }
    fn graphql(&self, _: &bug::Credential, _: &serde_json::Value) -> bug::Reply { panic!("no network") }
}
#[test]
fn p1_54_torn_sent_receipts_preserve_good_receipts_retries_and_cancellation() {
    use std::io::Write;
    let s = Scratch::new();
    let transport = ReceiptFixture(AtomicUsize::new(0));
    let outbox = bug::Outbox::open(&s.0);
    assert!(matches!(outbox.send(&public(), &Cred, &transport, 1).unwrap(), bug::Delivery::Sent(_)));
    assert!(matches!(outbox.send(&public(), &Cred, &transport, 2).unwrap(), bug::Delivery::Waiting { .. }));
    let path = s.0.join("sent.jsonl");
    fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(b"{\"title\":\"torn \xe2").unwrap();
    let reopened = bug::Outbox::open(&s.0);
    assert_eq!(reopened.pending().unwrap().len(), 1);
    assert_eq!(reopened.sent().unwrap().len(), 1);
    assert!(matches!(reopened.send_due(&Cred, &transport, u64::MAX).unwrap().as_slice(), [bug::Delivery::Sent(_)]));
    assert_eq!(reopened.sent().unwrap().len(), 2, "the next receipt must start on its own line");
    assert!(reopened.send_due(&Cred, &transport, u64::MAX).unwrap().is_empty());
    let bug::Delivery::Waiting { id, .. } = reopened.send(&public(), &Cred, &transport, 4).unwrap() else { panic!("fourth attempt must wait") };
    fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(b"\xff").unwrap();
    assert!(matches!(reopened.withdraw(&id).unwrap(), bug::Withdrawn::Canceled));
    assert_eq!(reopened.sent().unwrap().len(), 2);
    assert!(reopened.pending().unwrap().is_empty());
    assert_eq!(transport.0.load(Ordering::SeqCst), 4);
    assert!(fs::read(path).unwrap().contains(&0xe2), "damaged bytes are retained");
}
