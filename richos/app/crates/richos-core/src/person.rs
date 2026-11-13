//! THE USER, ONCE, ACROSS ALL THEIR COMPANIES: their own details (name, how to refer to them,
//! their public profile links), in one file of the user's central folder beside their
//! instructions, `person.md`.
//!
//! WHY (CEO, through the lead's answer to esc-20261112T092920Z-d4db39b0): a second or third
//! company is asked only what is new ("it would be considered retarded if Rich were to ask the
//! exact same 4 questions for the second and the third company"), and a CEO with several
//! current companies keeps them apart. The user is one person, so these details belong to the
//! person and every company's thread may read them. Company facts, notes and answers stay scoped
//! to their company (`company.rs`; `company_layer_tests`,
//! `a_thread_never_receives_another_companys_file`).
//!
//! Rich writes it through `save_person` (`onboarding_tools.rs`), under the same visible-turn
//! grant as the company notes; every priming turn carries it (`spine.rs` `company_block`). What
//! goes in it is the onboarding skill's call. No setting and no screen.

use std::path::{Path, PathBuf};

pub const FILENAME: &str = "person.md";
/// A person's own details are a few lines; the cap keeps company facts from moving here.
pub const BUDGET_BYTES: usize = 2048;

pub fn path(central_root: &Path) -> PathBuf {
    central_root.join(FILENAME)
}

/// The details on disk, or None when there are none yet (or the file is unreadable).
pub fn read(central_root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path(central_root)).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Replace the user's details, written atomically and read back. Returns the bytes written.
pub fn save(central_root: &Path, details: &str) -> Result<usize, String> {
    let details = details.trim();
    if details.is_empty() {
        return Err("Give the user's details. Nothing was saved.".into());
    }
    let body = format!("{details}\n");
    if body.len() > BUDGET_BYTES {
        return Err(format!("The user's details are {} bytes, over {BUDGET_BYTES}: keep only their name, how to refer \
            to them and their profile links; company facts belong in that company's notes. Nothing was saved.", body.len()));
    }
    std::fs::create_dir_all(central_root).map_err(|e| format!("The user's details could not be saved ({e}). Nothing was saved."))?;
    crate::doctrine::write_verified(&path(central_root), &body)
        .map_err(|e| format!("The user's details could not be saved ({e}). Nothing was saved."))?;
    match read(central_root) {
        Some(text) if text == details => Ok(body.len()),
        _ => Err("The user's details were written but did not read back. Treat them as not saved.".into()),
    }
}

/// The priming turn's view: where the details live and what they say (null text when none yet).
pub fn priming(central_root: &Path) -> serde_json::Value {
    serde_json::json!({"path": path(central_root), "text": read(central_root)})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("richos-person-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn details_are_saved_read_back_and_replaced_whole() {
        let root = tmp();
        assert_eq!(read(&root), None);
        assert_eq!(priming(&root)["text"], serde_json::Value::Null);
        save(&root, "Dana Whitfield, she/her\nLinkedIn: https://www.linkedin.com/in/dana/").unwrap();
        assert!(read(&root).unwrap().starts_with("Dana Whitfield"));
        save(&root, "Dana Whitfield, she/her").unwrap();
        assert_eq!(read(&root).as_deref(), Some("Dana Whitfield, she/her"));
        assert_eq!(priming(&root)["text"], "Dana Whitfield, she/her");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn empty_or_oversized_details_are_refused_and_nothing_changes() {
        let root = tmp();
        save(&root, "Dana").unwrap();
        assert!(save(&root, "  ").is_err());
        assert!(save(&root, &"x".repeat(BUDGET_BYTES)).unwrap_err().contains("company facts belong"));
        assert_eq!(read(&root).as_deref(), Some("Dana"));
        std::fs::remove_dir_all(&root).ok();
    }
}
