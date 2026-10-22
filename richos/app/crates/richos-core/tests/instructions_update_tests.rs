//! **The CEO, 2026-10-22: when a new RichOS version ships improved Rich or Stu instructions,
//! every existing user gets them.** A copy the user never changed is updated by the app itself;
//! a changed one is asked about once (`instructions_update.rs`). OK's session is tested beside
//! it (`instructions_update::tests`), with a stand-in `claude`.
use richos_core::instructions_update::{accept, check, decline, rows, Update, RECORD};
use richos_core::user_settings::{provision, RICH, STU};
use std::path::{Path, PathBuf};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("richos-instructions-update-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_dir_all(&self.0) { eprintln!("{} was left behind: {e}", self.0.display()); }
    }
}

fn ship(shipped: &Path, rich: &str, stu: &str) {
    std::fs::create_dir_all(shipped.join("agents")).unwrap();
    std::fs::write(shipped.join("agents").join(RICH), rich).unwrap();
    std::fs::write(shipped.join("agents").join(STU), stu).unwrap();
}

fn read(path: PathBuf) -> String { std::fs::read_to_string(path).unwrap() }

/// The real engine's shipped plugin.
fn engine_app() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app") }

const V1: &str = "---\nname: rich\n---\n\nYou are Rich.\n\nAnswer briefly.\n";
const V2: &str = "---\nname: rich\n---\n\nYou are Rich.\n\nAnswer briefly.\n\nAn improved rule.\n";
const V3: &str = "---\nname: rich\n---\n\nYou are Rich.\n\nAnswer briefly.\n\nAn improved rule.\n\nAnother one.\n";

#[test]
fn a_copy_the_user_never_changed_gets_the_new_version_silently() {
    let s = Scratch::new();
    let (folder, shipped) = (s.0.join("central"), s.0.join("shipped"));
    ship(&shipped, V1, "Stu one.\n");
    provision(&folder, &shipped).unwrap();
    assert!(folder.join(RECORD).is_file(), "a first-run copy must record what it was made from");
    ship(&shipped, V2, "Stu two.\n");
    provision(&folder, &shipped).unwrap();
    assert_eq!(read(folder.join(RICH)), V2, "an unchanged rich.md was not updated");
    assert_eq!(read(folder.join(STU)), "Stu two.\n", "an unchanged stu.md was not updated");
    assert!(check(&folder, &shipped).unwrap().is_empty(), "nothing is asked about an unchanged copy");
}

#[test]
fn a_changed_copy_is_asked_about_and_ok_writes_the_update_keeping_the_users_lines() {
    let s = Scratch::new();
    let (folder, shipped) = (s.0.join("central"), s.0.join("shipped"));
    ship(&shipped, V1, "Stu.\n");
    provision(&folder, &shipped).unwrap();
    let yours = format!("{V1}My own line.\n");
    std::fs::write(folder.join(RICH), &yours).unwrap();
    // No new version: a changed copy is the user's and nothing is asked.
    assert!(check(&folder, &shipped).unwrap().is_empty());
    ship(&shipped, V2, "Stu.\n");
    provision(&folder, &shipped).unwrap();
    assert_eq!(read(folder.join(RICH)), yours, "a changed copy was written without asking");
    assert_eq!(check(&folder, &shipped).unwrap(), vec![RICH], "the changed copy is to be asked about");

    let merged = format!("{V2}My own line.\n");
    let updated = accept(&folder, &shipped, RICH, |update| {
        assert_eq!(update, &Update { name: RICH, started_from: V1.into(), yours: yours.clone(), new: V2.into() });
        Ok((merged.clone(), "Added the improved rule; your line is kept.".into()))
    }).unwrap();
    assert_eq!(updated.said, "Added the improved rule; your line is kept.");
    assert_eq!(updated.rows, rows(V1, &yours, &merged), "the screen gets the file before and after");
    let now = read(folder.join(RICH));
    assert!(now.contains("My own line.") && now.contains("An improved rule."), "{now}");
    assert!(check(&folder, &shipped).unwrap().is_empty(), "the question came back after OK");

    // The next version starts from the one the update was made for.
    ship(&shipped, V3, "Stu.\n");
    assert_eq!(check(&folder, &shipped).unwrap(), vec![RICH]);
    accept(&folder, &shipped, RICH, |update| {
        assert_eq!(update.started_from, V2);
        Ok((format!("{V3}My own line.\n"), "Done.".into()))
    }).unwrap();
}

#[test]
fn no_leaves_the_file_as_it_was_and_is_not_asked_again_until_a_later_version() {
    let s = Scratch::new();
    let (folder, shipped) = (s.0.join("central"), s.0.join("shipped"));
    ship(&shipped, V1, "Stu.\n");
    provision(&folder, &shipped).unwrap();
    let yours = format!("{V1}My own line.\n");
    std::fs::write(folder.join(RICH), &yours).unwrap();
    ship(&shipped, V2, "Stu.\n");
    assert_eq!(check(&folder, &shipped).unwrap(), vec![RICH]);
    decline(&folder, &shipped).unwrap();
    assert_eq!(read(folder.join(RICH)), yours, "No changed the file");
    for _ in 0..2 {
        provision(&folder, &shipped).unwrap();
        assert!(check(&folder, &shipped).unwrap().is_empty(), "the question came back for the same version");
    }
    assert_eq!(read(folder.join(RICH)), yours);
    ship(&shipped, V3, "Stu.\n");
    assert_eq!(check(&folder, &shipped).unwrap(), vec![RICH], "a later version must ask again");
    accept(&folder, &shipped, RICH, |update| {
        assert_eq!(update.started_from, V1, "after No the starting point is still the one the copy was made from");
        Ok((format!("{V3}My own line.\n"), "Done.".into()))
    }).unwrap();
}

/// Users who installed before the record existed (the CEO and Andreas among them) have copies
/// with no record: one that is exactly a version shipped earlier is unchanged.
#[test]
fn a_copy_from_before_the_record_that_is_an_earlier_shipped_version_is_unchanged() {
    let shipped = engine_app();
    let current_rich = read(shipped.join("agents").join(RICH));
    let current_stu = read(shipped.join("agents").join(STU));
    let oldest_rich = read(shipped.join("earlier/rich/1.txt"));
    let oldest_stu = read(shipped.join("earlier/stu/1.txt"));
    assert_ne!(oldest_rich, current_rich, "the oldest earlier version must differ for this to test anything");
    let s = Scratch::new();
    let folder = s.0.join("central");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join(RICH), &oldest_rich).unwrap();
    std::fs::write(folder.join(STU), &oldest_stu).unwrap();
    provision(&folder, &shipped).unwrap();
    assert_eq!(read(folder.join(RICH)), current_rich, "an unchanged earlier rich.md was not updated");
    assert_eq!(read(folder.join(STU)), current_stu, "an unchanged earlier stu.md was not updated");
    assert!(check(&folder, &shipped).unwrap().is_empty());

    // And one that is not exactly a shipped version is changed, starting from the nearest one.
    let s = Scratch::new();
    let folder = s.0.join("central");
    std::fs::create_dir_all(&folder).unwrap();
    let third = read(shipped.join("earlier/rich/3.txt"));
    let yours = format!("{third}My own line.\n");
    std::fs::write(folder.join(RICH), &yours).unwrap();
    provision(&folder, &shipped).unwrap();
    assert_eq!(read(folder.join(RICH)), yours);
    assert_eq!(check(&folder, &shipped).unwrap(), vec![RICH]);
    accept(&folder, &shipped, RICH, |update| {
        assert_eq!(update.started_from, third, "the starting point is the nearest shipped version");
        Ok((current_rich.clone(), "Done.".into()))
    }).unwrap();
}

/// The earlier versions are byte for byte what shipped: a copy is recognized only by matching one
/// exactly, and the commit's Oxford comma fixer once rewrote `stu/1` when it was a `.md` file
/// (blob 1e67c1d75 became eca4e8016). SHA-256 of each shipped blob, oldest first.
#[test]
fn the_earlier_versions_are_exactly_what_shipped() {
    use sha2::Digest;
    let pinned = [
        ("rich/1.txt", "e11987ec03915b2f8f7089a4e0c8cca29108be64953ce520cff624a33bdc97c9"),
        ("rich/2.txt", "ef7b83544f44bc826cbc7f2429535e3104877e5867f3e10966df915e37962230"),
        ("rich/3.txt", "50af4b3a7da59c3a8961b514e09d3e8109d5c4ed0c1608fe498011714459076c"),
        ("rich/4.txt", "e0e141c273410f63eb4af7f1f872a4da5c7f39033dfde257bb724e510bf48ce9"),
        ("rich/5.txt", "3cd2775ba7b6bea7733b44f07fc4fe761d94a36fa4cae77eaac097f52047716f"),
        ("rich/6.txt", "42a44245caa639d793f713778b89413266ae0a4c733e15160dad614ec7373112"),
        ("stu/1.txt", "69b14fd925d38848f3132754b85cfa9388da1f9507f409039297a5e2efa4ad03"),
        ("stu/2.txt", "c7b9f76af83533501d8a42d4b9b80e96a15adb031d03b0669b9c0201d030f5bf"),
        ("stu/3.txt", "b80bfb1506bea6a86e1fd5c53c57c0c2d0395b700dbe13363e8dd8b44e0f8580"),
    ];
    let earlier = engine_app().join("earlier");
    for (file, sha) in pinned {
        let body = std::fs::read(earlier.join(file)).unwrap();
        assert_eq!(format!("{:x}", sha2::Sha256::digest(&body)), sha, "earlier/{file} is not the version that shipped");
    }
    let count = ["rich", "stu"].iter().map(|d| std::fs::read_dir(earlier.join(d)).unwrap().count()).sum::<usize>();
    assert_eq!(count, pinned.len(), "an earlier version was added or removed without being pinned here");
}

/// A link at the name is the user's choice of file and is never written through.
#[test]
fn a_linked_copy_is_never_updated() {
    let s = Scratch::new();
    let (folder, shipped) = (s.0.join("central"), s.0.join("shipped"));
    ship(&shipped, V2, "Stu.\n");
    std::fs::create_dir_all(&folder).unwrap();
    let elsewhere = s.0.join("my-rich.md");
    std::fs::write(&elsewhere, V1).unwrap();
    std::os::unix::fs::symlink(&elsewhere, folder.join(RICH)).unwrap();
    provision(&folder, &shipped).unwrap();
    assert_eq!(read(elsewhere), V1);
    assert!(check(&folder, &shipped).unwrap().is_empty());
}
