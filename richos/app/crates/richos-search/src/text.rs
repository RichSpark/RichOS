//! Case folding, query words and the two literal tests the text list is built on.
//!
//! **One fold, used everywhere a match is decided.** Text and query are lower-cased with
//! `char::to_lowercase` and every run of whitespace becomes one space, so a quoted span typed
//! as `"30 days"` matches a Markdown paragraph that wrapped between `30` and `days`. The fold
//! is applied to the query and to the cached document text the same way, which is the only
//! property the matching depends on; it is never used to slice an original string.

/// Lower-case and collapse whitespace runs to one space (leading and trailing removed).
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// A character that can sit inside a query word. Apostrophes, dots and hyphens are kept
/// inside a word (`don't`, `e.g`, `inv-2049`) and trimmed from its ends, the round-26
/// design's rule.
fn word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '\'' | '’' | '.' | '-')
}

fn trim_word(word: &str) -> &str {
    word.trim_matches(|c: char| matches!(c, '\'' | '’' | '.' | '-'))
}

/// Words too common to say what a document is about, so a vague question does not match
/// every file through "the" or "on". Declared in design round 26 (`index.html`, `STOP`) and
/// copied from it verbatim; the PRD leaves tokenization open. A query made only of these
/// words keeps them, so it still searches.
pub const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "that", "this", "those", "these", "where", "when", "which", "who", "whom",
    "i", "me", "my", "we", "us", "our", "you", "your", "it", "its", "is", "was", "were", "are",
    "be", "been", "being", "of", "on", "in", "to", "for", "and", "or", "with", "about", "chat",
    "chats", "thing", "things", "one", "how", "do", "did", "does", "doing", "what", "had", "have",
    "has", "at", "by", "from", "as", "so", "if", "not", "no", "up", "out", "like", "some",
    "there", "here", "they", "them", "their", "than", "then", "too", "very", "can", "could",
    "would", "should", "will", "just", "also", "again", "ever", "into", "onto",
];

/// The distinct words of already-folded text, in order of first appearance.
pub fn words_of(folded: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in folded.split(|c: char| !word_char(c)) {
        let word = trim_word(raw);
        if !word.is_empty() && !out.iter().any(|w| w == word) {
            out.push(word.to_string());
        }
    }
    out
}

fn is_alnum_at_end(prefix: &str) -> bool {
    prefix.chars().next_back().is_some_and(char::is_alphanumeric)
}

fn is_alnum_at_start(rest: &str) -> bool {
    rest.chars().next().is_some_and(char::is_alphanumeric)
}

/// Does folded `hay` contain folded `word` as a whole word, allowing a plural `s` or `es`?
///
/// Whole word: the characters on either side are not letters or digits, so `day` does not
/// match inside `today` or `daylight`. The plural allowance is the design's
/// (`index.html`, `wordRe`): `day` matches `days`, `price` matches `prices`.
pub fn has_word(hay: &str, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    for (at, _) in hay.match_indices(word) {
        if is_alnum_at_end(&hay[..at]) {
            continue;
        }
        let rest = &hay[at + word.len()..];
        let rest = rest.strip_prefix("es").filter(|r| !is_alnum_at_start(r)).or_else(|| rest.strip_prefix('s')).unwrap_or(rest);
        if !is_alnum_at_start(rest) {
            return true;
        }
    }
    false
}

/// How many of `words` occur in `hay` as whole words.
pub fn count_words(hay: &str, words: &[String]) -> usize {
    words.iter().filter(|w| has_word(hay, w)).count()
}

/// Does folded `hay` contain the folded span literally? Case differences are already folded
/// away; nothing else is relaxed (`"thirty days"` is not `"30 days"`, and `"INV-2049"` needs
/// its hyphen).
pub fn has_span(hay: &str, span: &str) -> bool {
    !span.is_empty() && hay.contains(span)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_lowercases_and_collapses_whitespace() {
        assert_eq!(fold("  Cancel within\n30   Days "), "cancel within 30 days");
        assert_eq!(fold("ÄRGER"), "ärger");
        assert_eq!(fold(""), "");
    }

    #[test]
    fn words_keep_inner_marks_and_drop_outer_ones() {
        assert_eq!(words_of("don't stop, inv-2049. e.g. 'quoted'"), vec!["don't", "stop", "inv-2049", "e.g", "quoted"]);
        assert_eq!(words_of("a a b"), vec!["a", "b"]);
    }

    #[test]
    fn whole_words_with_plurals_and_nothing_inside_other_words() {
        assert!(has_word("we refund within 30 days", "day"));
        assert!(has_word("new prices from may", "price"));
        assert!(has_word("two boxes", "box"));
        assert!(!has_word("today was fine", "day"));
        assert!(!has_word("daylight", "day"));
        assert!(!has_word("dayss", "day"));
        assert!(has_word("inv-2049 is late", "inv-2049"));
        assert!(has_word("end", "end"));
        assert!(!has_word("", "end"));
    }

    #[test]
    fn spans_are_literal() {
        let hay = fold("Cancel within 30\ndays of the first charge");
        assert!(has_span(&hay, &fold("30 days")));
        assert!(!has_span(&hay, &fold("thirty days")));
        assert!(has_span(&fold("Invoice INV-2049 paid"), &fold("INV-2049")));
        assert!(!has_span(&fold("Invoice INV 2049 paid"), &fold("INV-2049")));
    }
}
