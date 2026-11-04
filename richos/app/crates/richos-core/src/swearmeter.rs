//! Local lexical counts. No model, I/O, excerpts or message rewriting.
use serde::Serialize;
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

/// Increment when tokenization or dictionary membership changes. Totals are rebuilt
/// from the existing input histories at startup, never when display is enabled.
pub const DETECTOR_VERSION: u32 = 1;
pub const EVENT_SWEARMETER: &str = "rich://swearmeter";

/// Explicit English dictionary. A compound is one word even when multiple parts
/// match. These are vocabulary entries, not quotations from anyone's messages.
const WORDS: &[&str] = &[
    "arse", "arsehole", "arseholes", "ass", "asshole", "assholes", "bastard",
    "bastards", "bitch", "bitches", "bitching", "bollocks", "bullshit", "crap",
    "crappy", "cunt", "cunts", "dammit", "damn", "damned", "dick", "dickhead",
    "dickheads", "fuck", "fucked", "fucker", "fuckers", "fuckery", "fucking",
    "fucknig", "fucks", "fuckshit", "goddamn", "goddamned", "hell", "motherfucker",
    "motherfuckers", "motherfucking", "piss", "pissed", "pissing", "retard",
    "retarded", "retards", "shit", "shits", "shitshow", "shittier", "shittiest",
    "shitty", "wtf",
];

fn word_part(c: char) -> bool { c.is_alphanumeric() || is_combining_mark(c) }
fn joiner(c: char) -> bool { matches!(c, '-' | '\u{2010}' | '\u{2011}' | '\'' | '\u{2019}') }
fn matched(word: &str) -> bool {
    WORDS.binary_search(&word).is_ok()
        || word.split(joiner).any(|part| WORDS.binary_search(&part).is_ok())
}

/// Every whole matching word contributes one, including hyphenated/apostrophe
/// words. ASCII input avoids normalization allocation. Unicode uses the same
/// standard-library caseless mapping as the existing private-name matcher.
pub fn count(text: &str) -> u64 {
    if text.is_ascii() && !text.bytes().any(|b| b.is_ascii_uppercase()) {
        return count_normalized(text);
    }
    let normalized: String = text.nfkc()
        .flat_map(|c| c.to_lowercase().flat_map(char::to_uppercase).flat_map(char::to_lowercase))
        .collect();
    count_normalized(&normalized)
}

fn count_normalized(text: &str) -> u64 {
    let mut chars = text.char_indices().peekable();
    let mut start = None;
    let mut previous_part = false;
    let mut total = 0;
    while let Some((at, c)) = chars.next() {
        let part = word_part(c);
        let internal = joiner(c) && previous_part
            && chars.peek().is_some_and(|(_, next)| word_part(*next));
        if part {
            start.get_or_insert(at);
        } else if !internal {
            if let Some(from) = start.take() {
                total += u64::from(matched(&text[from..at]));
            }
        }
        previous_part = part;
    }
    if let Some(from) = start { total += u64::from(matched(&text[from..])); }
    total
}

/// Only numeric aggregates cross the app bridge. Missing history is never a
/// complete zero. `revision` orders snapshots from both durable input sources.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    pub detector_version: u32,
    pub count: u64,
    pub unknown_card_inputs: u64,
    pub unknown_message_inputs: u64,
    pub incomplete_history: bool,
    pub available: bool,
    pub revision: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_is_sorted_unique_and_every_entry_matches() {
        assert!(WORDS.windows(2).all(|pair| pair[0] < pair[1]));
        for word in WORDS { assert_eq!(count(word), 1, "{word}"); }
    }

    #[test]
    fn one_word_counts_once_and_separate_repetitions_count_separately() {
        for (text, expected) in [
            ("fuckshit", 1), ("fucking-shit", 1), ("fucking shit", 2),
            ("fuck-fuck-fuck", 1), ("fuck fuck fuck", 3), ("FUCK! Shit?", 2),
            ("assistant class pass shell", 0), ("retarded-ass", 1),
            ("shitshow, fucknig", 2), ("\"fuck\"", 1), ("fuck\u{2014}shit", 2),
            ("motherfucker", 1), ("for fuck's sake", 1), ("shit’s broken", 1),
            ("class's pass’s", 0), ("fuck--shit", 2), ("-fuck-", 1),
            ("class-assistant", 0), ("fuck42", 0), ("helloworld", 0),
        ] { assert_eq!(count(text), expected, "{text}"); }
    }

    #[test]
    fn unicode_normalization_case_and_internal_joiners_are_consistent() {
        for text in ["ＦＵＣＫ", "FUCK", "fuck‐shit", "fuck‑shit", "ſhit", "aß"] {
            assert_eq!(count(text), 1, "{text}");
        }
        assert_eq!(count("fu\u{301}ck"), 0); // Accents are not silently deleted.
        assert_eq!(count("class’s"), 0);
        assert_eq!(count(""), 0);
    }
}
