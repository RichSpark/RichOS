//! **WHICH RICHOS IS INSTALLED, READ WITHOUT RUNNING IT** (PRD §3), and the first screen's
//! installation line (round 39 §1).
//!
//! The two known application folders, `/Applications` and `~/Applications`, are looked at for
//! `RichOS.app`; its `Contents/Info.plist` is read (bounded) for the bundle identifier and
//! versions. RichOS is never executed, its settings are never loaded, and a missing or unreadable
//! bundle is evidence, not an error.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Where an app was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Place {
    /// `/Applications`.
    Applications,
    /// `~/Applications`.
    UserApplications,
}

impl Place {
    pub fn words(self) -> &'static str {
        match self {
            Place::Applications => "Applications",
            Place::UserApplications => "your user Applications folder",
        }
    }
    pub fn folder(self, home: &Path) -> PathBuf {
        match self {
            Place::Applications => PathBuf::from("/Applications"),
            Place::UserApplications => home.join("Applications"),
        }
    }
}

/// One installed copy of an app, as its bundle describes itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Found {
    pub place: Place,
    /// `CFBundleShortVersionString`, when readable.
    pub version: Option<String>,
    /// `CFBundleVersion`, when readable.
    pub build: Option<String>,
    /// Whether the bundle says it is the app looked for (its identifier matched).
    pub identity_ok: bool,
}

impl Found {
    /// "1.2.1 (nightly)" or "1.2.1", or `None` when the version could not be read.
    pub fn version_words(&self) -> Option<String> {
        let v = self.version.as_ref()?;
        Some(match v.split_once("-nightly") {
            Some((base, _)) => format!("{base} (nightly, {v})"),
            None => v.clone(),
        })
    }
}

/// The bundle's Info.plist, read (64 KiB at most, 2 seconds at most) and parsed.
pub fn read_info(app: &Path) -> Option<plist::Dictionary> {
    let read = crate::util::bounded_head(&app.join("Contents/Info.plist"), 64 * 1024, std::time::Duration::from_secs(2)).ok()?;
    if read.truncated {
        return None;
    }
    plist::from_bytes::<plist::Value>(&read.bytes).ok()?.into_dictionary()
}

/// Every copy of `bundle_name` (`RichOS.app`) in the two known folders, with identifier
/// `identifier`. A folder entry that is there but not readable is still listed, unread.
pub fn find(home: &Path, bundle_name: &str, identifier: &str) -> Vec<Found> {
    let mut out = Vec::new();
    for place in [Place::Applications, Place::UserApplications] {
        let app = place.folder(home).join(bundle_name);
        match std::fs::symlink_metadata(&app) {
            Ok(m) if m.is_dir() => {}
            _ => continue,
        }
        let info = read_info(&app);
        let get = |k: &str| info.as_ref().and_then(|d| d.get(k)).and_then(|v| v.as_string()).map(str::to_string);
        out.push(Found { place, version: get("CFBundleShortVersionString"), build: get("CFBundleVersion"), identity_ok: get("CFBundleIdentifier").as_deref() == Some(identifier) });
    }
    out
}

/// The installed RichOS copies.
pub fn richos(home: &Path) -> Vec<Found> {
    find(home, "RichOS.app", crate::RICHOS_IDENTIFIER)
}

/// Round 39's installation line on the first screen, by what was found, never claiming a
/// repair.
pub fn installation_line(found: &[Found]) -> String {
    let ours: Vec<&Found> = found.iter().filter(|f| f.identity_ok || f.version.is_none()).collect();
    match ours.as_slice() {
        [] => "RichOS was not found in Applications or in your user Applications folder. You can still describe what happened; the report will say RichOS was not found.".into(),
        [one] => match &one.version {
            Some(v) => format!("Found RichOS {} in {}.", display_version(v), one.place.words()),
            None => format!("RichOS is in {}, but its version could not be read. The report will say so.", one.place.words()),
        },
        [a, b, ..] => format!(
            "Found two copies of RichOS: {} in {} and {} in {}. RichDoctor will ask which one you were opening.",
            a.version.as_deref().map(display_version).unwrap_or_else(|| "an unreadable version".into()),
            a.place.words(),
            b.version.as_deref().map(display_version).unwrap_or_else(|| "an unreadable version".into()),
            b.place.words()
        ),
    }
}

/// "1.2.1-nightly.20261022.68" -> "1.2.1 (nightly 20261022.68)"; anything else as written.
pub fn display_version(v: &str) -> String {
    match v.split_once("-nightly.") {
        Some((base, n)) => format!("{base} (nightly {n})"),
        None => v.to_string(),
    }
}

/// Whether two copies were found, so the conversation's first question asks which one.
pub fn ambiguous(found: &[Found]) -> bool {
    found.iter().filter(|f| f.identity_ok).count() > 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle(root: &Path, version: &str, id: &str) {
        let c = root.join("RichOS.app/Contents");
        std::fs::create_dir_all(&c).unwrap();
        let plist = format!(r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>{id}</string><key>CFBundleShortVersionString</key><string>{version}</string><key>CFBundleVersion</key><string>{version}</string></dict></plist>"#);
        std::fs::write(c.join("Info.plist"), plist).unwrap();
    }

    #[test]
    fn copies_are_found_by_bundle_metadata_and_described_honestly() {
        let home = std::env::temp_dir().join(format!("rd-install-{}", crate::util::hex(&crate::util::random_bytes(4).unwrap())));
        assert_eq!(richos(&home).iter().filter(|f| f.place == Place::UserApplications).count(), 0);
        bundle(&home.join("Applications"), "1.1.0", crate::RICHOS_IDENTIFIER);
        let found: Vec<Found> = richos(&home).into_iter().filter(|f| f.place == Place::UserApplications).collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].version.as_deref(), Some("1.1.0"));
        assert!(found[0].identity_ok);
        assert_eq!(installation_line(&found), "Found RichOS 1.1.0 in your user Applications folder.");
        std::fs::write(home.join("Applications/RichOS.app/Contents/Info.plist"), "not a plist").unwrap();
        let unreadable: Vec<Found> = richos(&home).into_iter().filter(|f| f.place == Place::UserApplications).collect();
        assert_eq!(installation_line(&unreadable), "RichOS is in your user Applications folder, but its version could not be read. The report will say so.");
        std::fs::remove_dir_all(&home).unwrap();
        assert!(installation_line(&[]).starts_with("RichOS was not found in Applications or in your user Applications folder."));
        let two = [
            Found { place: Place::Applications, version: Some("1.2.1".into()), build: None, identity_ok: true },
            Found { place: Place::UserApplications, version: Some("1.1.0".into()), build: None, identity_ok: true },
        ];
        assert_eq!(installation_line(&two), "Found two copies of RichOS: 1.2.1 in Applications and 1.1.0 in your user Applications folder. RichDoctor will ask which one you were opening.");
        assert!(ambiguous(&two));
        assert_eq!(display_version("1.2.1-nightly.68"), "1.2.1 (nightly 68)");
    }
}
