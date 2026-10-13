//! What the CEO typed, split into the words that rank a document and the quoted spans that
//! can be required (search PRD v3.1, "Two independent checkboxes").
//!
//! *"Tokenize the case-folded query into words; quotes group a literal span but their words
//! also participate in ordinary matching."* Straight quotes and the typographic pair `“ ”`
//! both group, so a query pasted from a document behaves like one typed by hand. A quote mark
//! with no partner is reported in [`ParsedQuery::unmatched_quote`]; the caller refuses to
//! guess when Require quoted text is checked (*"Malformed quotes must not silently relax a
//! checked requirement."*).
use crate::text::{fold, words_of, STOP_WORDS};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedQuery {
    /// Distinct folded words, stop words removed unless nothing else is left.
    pub words: Vec<String>,
    /// Distinct folded quoted spans, in the order typed. Empty spans (`""`) are dropped.
    pub spans: Vec<String>,
    /// An odd number of quote marks: the last one has no partner.
    pub unmatched_quote: bool,
}

impl ParsedQuery {
    /// Nothing to search for: the palette shows its hint instead of results.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.spans.is_empty()
    }
}

fn is_quote(c: char) -> bool {
    matches!(c, '"' | '“' | '”')
}

pub fn parse(query: &str) -> ParsedQuery {
    let mut spans: Vec<String> = Vec::new();
    let mut inside = false;
    let mut current = String::new();
    let mut marks = 0usize;
    for c in query.chars() {
        if is_quote(c) {
            marks += 1;
            if inside {
                let span = fold(&current);
                if !span.is_empty() && !spans.contains(&span) {
                    spans.push(span);
                }
                current.clear();
            }
            inside = !inside;
        } else if inside {
            current.push(c);
        }
    }
    // Every word, quoted or not, also ranks ordinarily; quote marks are word separators.
    let folded = fold(&query.chars().map(|c| if is_quote(c) { ' ' } else { c }).collect::<String>());
    let all = words_of(&folded);
    let content: Vec<String> = all.iter().filter(|w| !STOP_WORDS.contains(&w.as_str())).cloned().collect();
    let words = if content.is_empty() && spans.is_empty() { all } else { content };
    ParsedQuery { words, spans, unmatched_quote: marks % 2 == 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_spans_also_rank_as_words() {
        let q = parse("cancel \"30 days\"");
        assert_eq!(q.spans, vec!["30 days"]);
        assert_eq!(q.words, vec!["cancel", "30", "days"]);
        assert!(!q.unmatched_quote);
    }

    #[test]
    fn two_spans_and_typographic_quotes() {
        let q = parse("“pricing” \"Support  Cost\"");
        assert_eq!(q.spans, vec!["pricing", "support cost"]);
        assert_eq!(q.words, vec!["pricing", "support", "cost"]);
    }

    #[test]
    fn identifiers_keep_their_hyphen() {
        let q = parse("\"INV-2049\"");
        assert_eq!(q.spans, vec!["inv-2049"]);
        assert_eq!(q.words, vec!["inv-2049"]);
    }

    #[test]
    fn stop_words_go_unless_they_are_all_there_is() {
        assert_eq!(parse("the app felt slow on first open").words, vec!["app", "felt", "slow", "first", "open"]);
        assert_eq!(parse("the who").words, vec!["the", "who"]);
        assert!(parse("   ").is_empty());
        assert!(parse("\"\"").is_empty());
    }

    #[test]
    fn an_unpaired_quote_is_reported_and_its_text_still_ranks() {
        let q = parse("cancel \"30 days");
        assert!(q.unmatched_quote);
        assert!(q.spans.is_empty());
        assert_eq!(q.words, vec!["cancel", "30", "days"]);
    }
}
