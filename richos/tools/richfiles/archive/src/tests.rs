use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use super::{Result, archive, command};

fn run(arguments: &[&OsStr]) -> Result<serde_json::Value> {
    command(arguments)
}

fn setup(root: &Path) -> Result<()> {
    fs::create_dir(root.join("Project"))?;
    fs::create_dir(root.join("Project/Empty"))?;
    fs::write(root.join("Project/Photo.png"), b"synthetic photo bytes")?;
    fs::write(root.join("Notes.txt"), b"notes")?;
    Ok(())
}

fn verify_no_stage(root: &Path) -> Result<()> {
    for entry in fs::read_dir(root)? {
        assert!(
            !entry?
                .file_name()
                .to_string_lossy()
                .starts_with(".richfiles-zip-")
        );
    }
    Ok(())
}

#[test]
fn creates_lists_and_extracts_an_ordinary_archive() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    setup(root)?;
    let notes = root.join("Notes.txt");
    let project = root.join("Project");
    let created = run(&[
        OsStr::new("create"),
        root.as_os_str(),
        OsStr::new("Package.zip"),
        OsStr::new("--"),
        project.as_os_str(),
        notes.as_os_str(),
    ])?;
    let package = root.join("Package.zip");
    assert_eq!(created["path"], package.canonicalize()?.to_str().unwrap());
    let listing = run(&[OsStr::new("list"), package.as_os_str()])?;
    let names: Vec<&str> = listing
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["Project", "Project/Empty", "Project/Photo.png", "Notes.txt"]
    );
    assert_eq!(listing[0]["isDirectory"], true);
    assert_eq!(listing[2]["size"], 21);
    assert_eq!(listing[3]["index"], 3);

    run(&[
        OsStr::new("extract"),
        package.as_os_str(),
        root.as_os_str(),
        OsStr::new("All"),
    ])?;
    assert!(root.join("All/Project/Empty").is_dir());
    assert_eq!(
        fs::read(root.join("All/Project/Photo.png"))?,
        b"synthetic photo bytes"
    );
    assert_eq!(fs::read(root.join("All/Notes.txt"))?, b"notes");

    run(&[
        OsStr::new("extract"),
        package.as_os_str(),
        root.as_os_str(),
        OsStr::new("Selected"),
        OsStr::new("--"),
        OsStr::new("0"),
    ])?;
    assert_eq!(
        fs::read(root.join("Selected/Project/Photo.png"))?,
        b"synthetic photo bytes"
    );
    assert!(root.join("Selected/Project/Empty").is_dir());
    assert!(!root.join("Selected/Notes.txt").exists());
    assert_eq!(fs::read(notes)?, b"notes");
    assert_eq!(
        fs::read(project.join("Photo.png"))?,
        b"synthetic photo bytes"
    );
    verify_no_stage(root)
}

#[test]
fn existing_targets_and_changed_selections_leave_originals_in_place() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    setup(root)?;
    let notes = root.join("Notes.txt");
    let archive = archive::create(root, "Package.zip", std::slice::from_ref(&notes))?;
    let original_archive = fs::read(&archive)?;
    assert!(archive::create(root, "Package.zip", std::slice::from_ref(&notes)).is_err());
    assert_eq!(fs::read(&archive)?, original_archive);

    let occupant = root.join("Occupied");
    fs::create_dir(&occupant)?;
    fs::write(occupant.join("Keep.txt"), b"existing content")?;
    assert!(archive::extract(&archive, root, "Occupied", &[]).is_err());
    assert_eq!(fs::read(occupant.join("Keep.txt"))?, b"existing content");
    assert!(!occupant.join("Notes.txt").exists());
    assert!(archive::extract(&archive, root, "InvalidSelection", &[1]).is_err());
    assert!(!root.join("InvalidSelection").exists());
    assert_eq!(fs::read(notes)?, b"notes");
    verify_no_stage(root)
}

#[test]
fn a_link_in_the_source_tree_stops_creation_before_publication() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    setup(root)?;
    symlink(root.join("Notes.txt"), root.join("Project/Alias.txt"))?;
    assert!(archive::create(root, "Package.zip", &[root.join("Project")]).is_err());
    assert!(!root.join("Package.zip").exists());
    assert_eq!(fs::read(root.join("Notes.txt"))?, b"notes");
    verify_no_stage(root)
}

#[test]
fn writing_inside_a_selected_folder_does_not_include_its_staging_output() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    setup(root)?;
    let project = root.join("Project");
    let output = archive::create(&project, "Package.zip", std::slice::from_ref(&project))?;
    let listing = archive::list(&output)?;
    assert_eq!(
        listing
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>(),
        ["Project", "Project/Empty", "Project/Photo.png"]
    );
    verify_no_stage(&project)
}

#[test]
fn tar_with_a_link_stops_before_creating_the_destination() -> Result<()> {
    let fixture = tempfile::tempdir()?;
    let root = fixture.path();
    setup(root)?;
    symlink("Photo.png", root.join("Project/Alias.png"))?;
    let archive = root.join("Package.tar");
    let status = std::process::Command::new("/usr/bin/tar")
        .env("COPYFILE_DISABLE", "1")
        .args([
            OsStr::new("-cf"),
            archive.as_os_str(),
            OsStr::new("Project"),
        ])
        .current_dir(root)
        .status()?;
    assert!(status.success());

    let destination = root.join("Unpacked");
    let result = run(&[
        OsStr::new("extract"),
        archive.as_os_str(),
        root.as_os_str(),
        OsStr::new("Unpacked"),
    ]);
    assert!(result.unwrap_err().to_string().contains("link"));
    assert!(!destination.exists());
    assert_eq!(
        fs::read(root.join("Project/Photo.png"))?,
        b"synthetic photo bytes"
    );
    verify_no_stage(root)
}
