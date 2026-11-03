//! Hunt v5 G7: recovery after interrupted writes.
#[cfg(test)]
use crate::spend::{ledger::Ledger, Request};
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
fn request(id: &str) -> Request {
    Request::of_transcript_line(serde_json::json!({"type":"assistant", "timestamp":"2026-10-13T06:00:00Z",
        "message":{"id":id,"model":"claude-opus-5-5","usage":{"input_tokens":1000,"output_tokens":50}}}).to_string().as_bytes()).unwrap()
}
#[test]
fn p1_56_failed_totals_save_does_not_commit_seen_requests() {
    let s = Scratch::new();
    let ledger = Ledger::open(&s.0);
    let now = 1_791_871_200_000;
    let first = request("one");
    let second = request("two");
    ledger.add(vec![first.clone()], now);
    ledger.save(now, true);
    fs::create_dir(s.0.join("state.json.tmp")).unwrap();
    ledger.add(vec![second.clone()], now + 1);
    ledger.save(now + 1, true);
    assert_eq!(ledger.totals(0).requests, 2);
    drop(ledger);
    fs::remove_dir(s.0.join("state.json.tmp")).unwrap();
    let reopened = Ledger::open(&s.0);
    reopened.add(vec![first.clone(), second.clone()], now + 2);
    assert_eq!(reopened.totals(0).requests, 2, "replay must recover the unsaved request");
    reopened.save(now + 2, true);
    drop(reopened);
    let again = Ledger::open(&s.0);
    again.add(vec![first, second], now + 3);
    assert_eq!(again.totals(0).requests, 2, "a successful retry must not double count");
}

#[test]
fn p1_56_upgrade_checkpoints_the_old_totals_before_appending_ids() {
    let s = Scratch::new();
    let now = 1_791_871_200_000;
    let ledger = Ledger::open(&s.0);
    ledger.add(vec![request("one")], now);
    ledger.save(now, true);
    drop(ledger);
    let path = s.0.join("state.json");
    let mut legacy: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    legacy.as_object_mut().unwrap().remove("seen_len");
    fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    let upgraded = Ledger::open(&s.0);
    fs::create_dir(s.0.join("state.json.tmp")).unwrap();
    upgraded.add(vec![request("two")], now + 1);
    upgraded.save(now + 1, true);
    assert_eq!(fs::metadata(s.0.join("seen.bin")).unwrap().len(), 16, "no new ids before the old checkpoint is saved");
    fs::remove_dir(s.0.join("state.json.tmp")).unwrap();
    upgraded.save(now + 2, true);
    drop(upgraded);
    let reopened = Ledger::open(&s.0);
    reopened.add(vec![request("one"), request("two")], now + 3);
    assert_eq!(reopened.totals(0).requests, 2);
}

#[test]
fn p1_56_uncommitted_partial_tail_is_ignored_and_replaced() {
    use std::io::Write;
    let s = Scratch::new();
    let now = 1_791_871_200_000;
    let ledger = Ledger::open(&s.0);
    ledger.add(vec![request("one")], now);
    ledger.save(now, true);
    drop(ledger);
    fs::OpenOptions::new().append(true).open(s.0.join("seen.bin")).unwrap().write_all(&[0xff; 19]).unwrap();
    let reopened = Ledger::open(&s.0);
    reopened.add(vec![request("one"), request("two")], now + 1);
    reopened.save(now + 1, true);
    assert_eq!(fs::metadata(s.0.join("seen.bin")).unwrap().len(), 32);
    drop(reopened);
    let again = Ledger::open(&s.0);
    again.add(vec![request("one"), request("two")], now + 2);
    assert_eq!(again.totals(0).requests, 2);
}
