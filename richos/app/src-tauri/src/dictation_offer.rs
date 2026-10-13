//! RICH'S ONE OFFER OF DICTATION, the app's half of it (round 19, state 4; dictation plan
//! revision 2, richos-hq `docs/plans/2026-10-08-dictation-anywhere.md`, section 2 row 4).
//!
//! The window shows the offer once, after Start on the setup sheet and once voice is ready
//! (`ui/main.js` `maybeOfferDictation`), and asks this file two things: may it be shown
//! (`dictation_offer`: dictation is there, it was never made, and which key to name), and
//! record that it was (`dictation_offer_shown`). The record is `offered` in
//! `<data dir>/dictation.json`, the one dictation settings file (plan section 1), so a relaunch
//! never shows it again.
//!
//! **Only the `offered` and `key` fields are read or written here, and every other field in the
//! file is kept as it is**: the file's full shape and its other fields belong to the dictation
//! settings store (slice 1). A write reads the file as a JSON object, sets `offered`, and
//! replaces the file atomically (a sibling renamed over it), so the tool never reads half a file.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// `<data dir>/dictation.json` (plan section 1).
pub const FILE_NAME: &str = "dictation.json";

/// What the window needs to decide whether to offer, and what key to name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OfferView {
    /// Dictation is there (`richos_core::dictation_ready`). `false` means never offer.
    pub ready: bool,
    /// The offer was made already.
    pub offered: bool,
    /// The chosen key as an F-number, 1 to 19; F1 when the file says nothing usable.
    pub key: u8,
}

fn path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE_NAME)
}

/// The file as a JSON object: an empty one when there is no file, an error when it cannot be
/// read or is not an object (never a silent "not offered" over a broken file).
fn read_object(data_dir: &Path) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    match std::fs::read(path(data_dir)) {
        Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(serde_json::Value::Object(map)) => Ok(map),
            Ok(_) => Err(format!("{FILE_NAME} is not a JSON object")),
            Err(e) => Err(format!("{FILE_NAME} is not readable: {e}")),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(serde_json::Map::new()),
        Err(e) => Err(format!("{FILE_NAME} could not be read: {e}")),
    }
}

/// May the offer be shown? A file that cannot be read answers "offered", so a broken file never
/// makes Rich repeat himself; the reason goes to the log.
pub fn view(data_dir: &Path, ready: bool) -> OfferView {
    match read_object(data_dir) {
        Ok(map) => {
            let offered = map.get("offered").and_then(|v| v.as_bool()).unwrap_or(false);
            let key = map
                .get("key")
                .and_then(|v| v.as_u64())
                .filter(|k| (1..=19).contains(k))
                .map(|k| k as u8)
                .unwrap_or(1);
            OfferView { ready, offered, key }
        }
        Err(why) => {
            eprintln!("[richos] dictation offer: not offered, because {why}");
            OfferView { ready, offered: true, key: 1 }
        }
    }
}

/// Record that the offer was shown: `offered: true`, every other field kept. Under the same
/// file lock the sheet and the tool write through (`store::update`), so this third writer loses
/// nothing of theirs and they lose nothing of its (review finding 3, 2026-10-08).
pub fn mark_shown(data_dir: &Path) -> Result<(), String> {
    set_field(data_dir, "offered", serde_json::Value::Bool(true))
}

// ---- THE "DICTATION FEATURE" CONVERSATION ------------------------------------------------
//
// The CEO, 2026-10-13 (richos-hq docs/ceo-input/2026-10-13_1, onboarding5.png): "Once
// dictation is setup, ... a new conversation should be automatically created titled
// "Dictation feature"", holding his text below with the key he actually picked. Created once:
// `featureThread` in the same file records the conversation's id.

/// The conversation's title, his words.
pub const FEATURE_TITLE: &str = "Dictation feature";

/// Rich's message in it, his words verbatim, "[selected key]" filled in with the chosen key.
pub fn feature_text(key: u8) -> String {
    format!(
        "Good news! Dictation is set up and enabled.\n\n\
         So, in any other app on this Mac (Mail, Slack Chrome/Safari etc.) you can now do this:\n\n\
         Click into where you want your words to appear, tap F{key}, say what you want written, then \
         tap it again. See how your words appear where your cursor is.\n\n\
         If you have any questions about this, we can discuss it here."
    )
}

/// The id of the conversation already created, if it was. An unreadable file answers "created",
/// for the reason [`view`] answers "offered": a broken file never makes Rich repeat himself.
pub fn feature_thread(data_dir: &Path) -> Option<String> {
    match read_object(data_dir) {
        Ok(map) => map.get("featureThread").and_then(|v| v.as_str()).map(str::to_string),
        Err(why) => {
            eprintln!("[richos] dictation feature conversation: not created, because {why}");
            Some(String::new())
        }
    }
}

/// Record the conversation that was created, every other field kept.
pub fn mark_feature_thread(data_dir: &Path, thread_id: &str) -> Result<(), String> {
    set_field(data_dir, "featureThread", serde_json::Value::String(thread_id.to_string()))
}

/// Set one field of the file, every other field kept, under the writers' shared lock.
fn set_field(data_dir: &Path, field: &str, value: serde_json::Value) -> Result<(), String> {
    let _held = crate::dictation::store::lock(data_dir)?;
    let mut map = read_object(data_dir)?;
    map.insert(field.to_string(), value);
    std::fs::create_dir_all(data_dir).map_err(|e| format!("the data folder could not be created: {e}"))?;
    let partial = data_dir.join(format!(".{FILE_NAME}.offer.{}", std::process::id()));
    let body = serde_json::to_vec_pretty(&serde_json::Value::Object(map)).map_err(|e| e.to_string())?;
    std::fs::write(&partial, body).map_err(|e| format!("{FILE_NAME} could not be written: {e}"))?;
    std::fs::rename(&partial, path(data_dir)).map_err(|e| {
        if let Err(left) = std::fs::remove_file(&partial) {
            eprintln!("[richos] dictation offer: the partial file was left behind: {left}");
        }
        format!("{FILE_NAME} could not be replaced: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("richos-dictation-offer-{name}-{}", std::process::id()));
        if d.exists() {
            std::fs::remove_dir_all(&d).expect("a leftover test folder is removable");
        }
        d
    }

    /// INVARIANT: no file is "never offered, F1"; once shown, the offer is never made again,
    /// relaunch or not, and the gate still decides whether it may be made at all.
    #[test]
    fn the_offer_is_made_once_and_remembered() {
        let d = dir("once");
        assert_eq!(view(&d, true), OfferView { ready: true, offered: false, key: 1 });
        assert_eq!(view(&d, false), OfferView { ready: false, offered: false, key: 1 });
        mark_shown(&d).unwrap();
        assert!(view(&d, true).offered, "a relaunch reads the same file");
        // Beside the file only the writers' shared lock (store::LOCK_NAME) may stay: no partial file.
        let mut leftovers: Vec<_> = std::fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name()).collect();
        leftovers.sort();
        assert_eq!(
            leftovers,
            vec![std::ffi::OsString::from(crate::dictation::store::LOCK_NAME), std::ffi::OsString::from(FILE_NAME)],
            "no partial file left"
        );
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// INVARIANT: the offer touches only `offered`; the dictation store's other fields survive,
    /// and the key it names is the chosen one.
    #[test]
    fn marking_the_offer_keeps_every_other_setting() {
        let d = dir("keep");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(path(&d), br#"{"on":true,"key":5,"model":"small.en","micAsked":true,"axAsked":false,"offered":false}"#).unwrap();
        assert_eq!(view(&d, true).key, 5);
        mark_shown(&d).unwrap();
        let after: serde_json::Value = serde_json::from_slice(&std::fs::read(path(&d)).unwrap()).unwrap();
        assert_eq!(
            after,
            serde_json::json!({"on":true,"key":5,"model":"small.en","micAsked":true,"axAsked":false,"offered":true})
        );
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// INVARIANT: a broken file is never read as "not offered", and is never overwritten.
    #[test]
    fn a_broken_file_offers_nothing_and_is_left_alone() {
        let d = dir("broken");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(path(&d), b"{not json").unwrap();
        assert!(view(&d, true).offered);
        assert!(mark_shown(&d).is_err());
        assert_eq!(std::fs::read(path(&d)).unwrap(), b"{not json");
        // A key outside F1..F19 names F1.
        std::fs::write(path(&d), br#"{"key":42}"#).unwrap();
        assert_eq!(view(&d, true).key, 1);
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// INVARIANT: the "Dictation feature" message is his text word for word, with the key he
    /// picked (onboarding5.png: he picked F2 and the app said F1).
    #[test]
    fn the_feature_message_is_his_text_with_the_picked_key() {
        assert_eq!(
            feature_text(2),
            "Good news! Dictation is set up and enabled.\n\n\
             So, in any other app on this Mac (Mail, Slack Chrome/Safari etc.) you can now do this:\n\n\
             Click into where you want your words to appear, tap F2, say what you want written, then tap it again. \
             See how your words appear where your cursor is.\n\n\
             If you have any questions about this, we can discuss it here."
        );
        assert_eq!(FEATURE_TITLE, "Dictation feature");
    }

    /// INVARIANT: the conversation is created once, the record keeps every other setting, and a
    /// broken file never creates it again.
    #[test]
    fn the_feature_conversation_is_recorded_once_and_keeps_every_other_setting() {
        let d = dir("feature");
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(feature_thread(&d), None);
        std::fs::write(path(&d), br#"{"on":true,"key":2,"offered":true}"#).unwrap();
        assert_eq!(feature_thread(&d), None);
        mark_feature_thread(&d, "thr_dictation").unwrap();
        assert_eq!(feature_thread(&d).as_deref(), Some("thr_dictation"));
        let after: serde_json::Value = serde_json::from_slice(&std::fs::read(path(&d)).unwrap()).unwrap();
        assert_eq!(after, serde_json::json!({"on":true,"key":2,"offered":true,"featureThread":"thr_dictation"}));
        std::fs::write(path(&d), b"{not json").unwrap();
        assert!(feature_thread(&d).is_some(), "a broken file is read as created");
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// INVARIANT: an ordinary settings write (the switch, the key, the accuracy, through
    /// `store::update`) keeps the record, so turning dictation off and on again never opens a
    /// second "Dictation feature" conversation (second review rv-20261013T173014Z, finding 1).
    #[test]
    fn a_settings_write_keeps_the_feature_conversation_record() {
        let d = dir("feature-kept");
        std::fs::create_dir_all(&d).unwrap();
        mark_feature_thread(&d, "thr_dictation").unwrap();
        crate::dictation::store::update(&d, |s| { s.on = false; s.key = 7; }).unwrap();
        crate::dictation::store::update(&d, |s| s.on = true).unwrap();
        assert_eq!(feature_thread(&d).as_deref(), Some("thr_dictation"));
        std::fs::remove_dir_all(&d).unwrap();
    }
}
