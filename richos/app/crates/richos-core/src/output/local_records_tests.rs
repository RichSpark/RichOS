//! Hunt v5 G7: recovery after interrupted writes.
#[cfg(test)]
use crate::output::{Actor, OutputStore, WriteRow, WriteSource};
use std::io::Write;
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
#[test]
fn p1_55_torn_utf8_does_not_block_good_output_or_later_appends() {
    let s = Scratch::new();
    let store = OutputStore::new(s.0.join("output"));
    let p = s.0.join("note.md");
    fs::write(&p, "good").unwrap();
    let row = |key: &str| WriteRow::witnessed(key.into(), "thread", Some("turn"), p.to_str().unwrap(), Actor::Rich, WriteSource::Tool, 1);
    store.append("thread", &[row("one")]).unwrap();
    let path = store.record_path("thread").unwrap();
    fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(b"{\"key\":\"torn\",\"path\":\"\xe2").unwrap();
    assert_eq!(store.project("thread").unwrap().count, 1);
    assert_eq!(store.read("thread").unwrap().corrupt, 1);
    assert_eq!(store.append("thread", &[row("one"), row("two")]).unwrap().len(), 1);
    let read = store.read("thread").unwrap();
    assert_eq!((read.rows.len(), read.corrupt), (2, 1));
    assert!(fs::read(path).unwrap().windows(1).any(|b| b == [0xe2]), "damaged bytes are preserved");
}
