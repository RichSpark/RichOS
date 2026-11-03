//! Hunt v5 G7: recovery after interrupted writes.
#[cfg(test)]
use crate::your_instructions;
use std::sync::{Arc, Barrier};
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
fn p1_61_independent_instruction_writers_keep_every_successful_rule() {
    let s = Scratch::new();
    let prefix = "Existing instruction.\n".repeat(30_000);
    your_instructions::save(&s.0, &prefix).unwrap();
    let barrier = Arc::new(Barrier::new(16));
    let writers: Vec<_> = (0..16).map(|n| {
        let (dir, gate) = (s.0.clone(), barrier.clone());
        std::thread::spawn(move || {
            gate.wait();
            your_instructions::add_rules(&dir, &[format!("Keep rule {n}.")]).unwrap();
        })
    }).collect();
    let results: Vec<_> = writers.into_iter().map(|writer| writer.join()).collect();
    for result in results { result.unwrap(); }
    let text = your_instructions::read(&s.0).unwrap();
    assert!(text.starts_with(&prefix));
    for n in 0..16 { assert_eq!(text.lines().filter(|l| *l == format!("- Keep rule {n}.")).count(), 1, "rule {n} was lost"); }
}

#[test]
fn p1_61_a_symlink_and_its_target_share_the_instruction_lock() {
    let s = Scratch::new();
    let alias = s.0.join("alias");
    fs::create_dir(&alias).unwrap();
    your_instructions::save(&s.0, &"Existing rule.\n".repeat(30_000)).unwrap();
    std::os::unix::fs::symlink(your_instructions::path(&s.0), your_instructions::path(&alias)).unwrap();
    let gate = Arc::new(Barrier::new(8));
    let writers: Vec<_> = (0..8).map(|n| {
        let dir = if n % 2 == 0 { s.0.clone() } else { alias.clone() };
        let gate = gate.clone();
        std::thread::spawn(move || {
            gate.wait();
            your_instructions::add_rules(&dir, &[format!("Alias rule {n}.")]).unwrap();
        })
    }).collect();
    let results: Vec<_> = writers.into_iter().map(|writer| writer.join()).collect();
    for result in results { result.unwrap(); }
    let text = your_instructions::read(&s.0).unwrap();
    for n in 0..8 { assert!(text.lines().any(|l| l == format!("- Alias rule {n}."))); }
    assert!(fs::symlink_metadata(your_instructions::path(&alias)).unwrap().file_type().is_symlink());
}
