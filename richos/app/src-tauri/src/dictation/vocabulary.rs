//! Company-scoped local settings and the Mac's English spelling safety check.
//! The app supplies registry identities. This module never discovers companies.

use objc2::{encode::{Encode, Encoding}, msg_send, runtime::{AnyClass, AnyObject, Bool}};
use richos_voice::dictation_vocabulary::{Invalid, Rule, Rules};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::{CStr, CString};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Company {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Saved {
    pub selected_company: Option<String>,
    pub by_company: BTreeMap<String, Vec<Rule>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub company: Company,
    pub rules: Vec<Rule>,
}

/// A transient publication from the owning app's registry/settings. The tool
/// consumes it; it neither writes nor maintains a separate company registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Publication {
    pub data_dir: String,
    pub companies: Vec<Company>,
    pub active: Option<Snapshot>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub companies: Vec<Company>,
    pub selected_company: Option<String>,
    pub rules: Vec<Rule>,
    pub error: Option<String>,
    pub preview_enabled: bool,
    pub preview_available: bool,
}

impl Saved {
    pub fn read(raw: &serde_json::Value) -> Result<Self, String> {
        if raw.is_null() {
            return Ok(Self::default());
        }
        serde_json::from_value(raw.clone()).map_err(|_| "This vocabulary couldn't be read. Your words will be written without spelling changes.".into())
    }

    pub fn snapshot(&self, companies: &[Company]) -> Option<Snapshot> {
        let id = self.selected_company.as_ref()?;
        let company = companies.iter().find(|c| &c.id == id)?.clone();
        let rules = self.by_company.get(id).cloned().unwrap_or_default();
        Some(Snapshot { company, rules })
    }
}

pub fn invalid_message(invalid: Invalid) -> &'static str {
    match invalid {
        Invalid::EmptySpelling => "Enter the correct spelling.",
        Invalid::UnsupportedCharacters => "Use a word or phrase without line breaks or code symbols in Heard as.",
        Invalid::CommonWord => "That is also an English word. Use a longer phrase in Heard as to avoid changing ordinary sentences.",
        Invalid::SpellingUnavailable => "I can't check that word safely right now. Use a longer phrase in Heard as.",
        Invalid::ConflictingTrigger => "Those words already have a different spelling rule. Edit the existing rule or use a different phrase.",
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Range {
    location: usize,
    length: usize,
}

// SAFETY: Foundation's NSRange is two NSUInteger values on supported 64-bit Macs.
unsafe impl Encode for Range {
    const ENCODING: Encoding = Encoding::Struct("_NSRange", &[usize::ENCODING, usize::ENCODING]);
}

fn classify(range: Range, units: usize, count: isize) -> Option<bool> {
    if count != 1 {
        return None;
    }
    if range.location == isize::MAX as usize && range.length == 0 {
        Some(true)
    } else if range.location == 0 && range.length == units {
        Some(false)
    } else {
        None
    }
}

/// Call on the main thread through bar::ask_main. No network operation, user
/// dictionary changes, learning, correction panel or language-default mutation.
/// Check lowercase and sentence-start case in every installed English variant:
/// e.g. lowercase monday alone is reported misspelled on some Macs.
pub fn common_word(word: &str) -> Option<bool> {
    // SAFETY: system thread-identity query with no arguments or side effects.
    if unsafe { libc::pthread_main_np() } == 0 {
        return None;
    }
    objc2::rc::autoreleasepool(|_| {
        let class = AnyClass::get(c"NSSpellChecker")?;
        let string_class = AnyClass::get(c"NSString")?;
        // SAFETY: documented AppKit/Foundation messages, on the main thread;
        // strings are retained by this autorelease pool until all calls return.
        unsafe {
            let checker: *mut AnyObject = msg_send![class, sharedSpellChecker];
            if checker.is_null() { return None; }
            let languages: *mut AnyObject = msg_send![checker, availableLanguages];
            if languages.is_null() { return None; }
            let count: usize = msg_send![languages, count];
            let tag: isize = msg_send![class, uniqueSpellDocumentTag];
            let verdict = (|| {
                let mut found = false;
                let lower = word.to_lowercase();
                let mut letters = lower.chars();
                let capitalized = letters.next()?.to_uppercase().collect::<String>() + letters.as_str();
                for i in 0..count {
                    let language: *mut AnyObject = msg_send![languages, objectAtIndex: i];
                    let utf8: *const std::ffi::c_char = msg_send![language, UTF8String];
                    if utf8.is_null() { return None; }
                    let label = CStr::from_ptr(utf8).to_str().ok()?;
                    if label != "en" && !label.starts_with("en_") { continue; }
                    found = true;
                    for spelling in [&lower, &capitalized] {
                        let bytes = CString::new(spelling.as_str()).ok()?;
                        let string: *mut AnyObject = msg_send![string_class, stringWithUTF8String: bytes.as_ptr()];
                        if string.is_null() { return None; }
                        let mut words: isize = -1;
                        let range: Range = msg_send![checker, checkSpellingOfString: string,
                            startingAt: 0isize, language: language, wrap: Bool::NO,
                            inSpellDocumentWithTag: tag, wordCount: &mut words];
                        match classify(range, spelling.encode_utf16().count(), words) {
                            Some(false) => {}
                            other => return other,
                        }
                    }
                }
                found.then_some(false)
            })();
            let _: () = msg_send![checker, closeSpellDocumentWithTag: tag];
            verdict
        }
    })
}

pub fn checked_on_main(app: &tauri::AppHandle, rules: Vec<Rule>) -> Result<Rules, String> {
    super::bar::ask_main(app, move || Rules::checked(&rules, common_word))
        .ok_or_else(|| invalid_message(Invalid::SpellingUnavailable).to_string())?
        .map_err(|e| invalid_message(e).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_snapshot_uses_only_the_explicit_registered_company_and_its_rules() {
        let a = Company { id: "a".into(), name: "Company A".into() };
        let b = Company { id: "b".into(), name: "Company B".into() };
        let mut saved = Saved::default();
        saved.by_company.insert("a".into(), vec![Rule { spelling: "A".into(), heard_as: "a team".into() }]);
        saved.by_company.insert("b".into(), vec![Rule { spelling: "B".into(), heard_as: "a team".into() }]);
        assert!(saved.snapshot(&[a.clone(), b.clone()]).is_none());
        saved.selected_company = Some("a".into());
        let snapshot = saved.snapshot(&[a, b.clone()]).unwrap();
        assert_eq!(snapshot.rules[0].spelling, "A");
        assert!(saved.snapshot(&[b]).is_none(), "company deletion never selects another dictionary");
        saved.by_company.clear();
        assert_eq!(snapshot.rules[0].spelling, "A", "the recording's snapshot is immutable");
    }

    #[test]
    fn malformed_optional_vocabulary_is_reported_and_not_interpreted_as_rules() {
        assert_eq!(Saved::read(&serde_json::Value::Null).unwrap(), Saved::default());
        assert!(Saved::read(&serde_json::json!({"byCompany":{"a":[{"spelling":42}]}})).is_err());
    }

    #[test]
    fn only_one_whole_misspelled_word_is_an_acceptable_single_word_alias() {
        assert_eq!(classify(Range { location: 0, length: 4 }, 4, 1), Some(false));
        assert_eq!(classify(Range { location: isize::MAX as usize, length: 0 }, 4, 1), Some(true));
        assert_eq!(classify(Range { location: 0, length: 2 }, 4, 1), None);
        assert_eq!(classify(Range { location: 0, length: 4 }, 4, -1), None);
        assert_eq!(classify(Range { location: 0, length: 4 }, 4, 0), None);
    }
}
