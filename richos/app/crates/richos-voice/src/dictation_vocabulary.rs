//! Explicit spelling corrections, applied once to recognized English text.
//! No recognizer prompts, learning, company inference or input/output logging.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rule {
    pub spelling: String,
    #[serde(default)]
    pub heard_as: String,
}

impl Rule {
    pub fn trigger(&self) -> &str {
        if self.heard_as.trim().is_empty() { self.spelling.trim() } else { self.heard_as.trim() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invalid {
    EmptySpelling,
    UnsupportedCharacters,
    CommonWord,
    SpellingUnavailable,
    ConflictingTrigger,
}

/// A recording owns an immutable, validated ruleset. The spelling callback returns
/// Some(true) for an English word, Some(false) for a whole misspelled word and
/// None for unavailable/uncertain service results. Unknown never means safe.
#[derive(Debug, Clone, Default)]
pub struct Rules(Vec<Rule>);

impl Rules {
    pub fn checked(rules: &[Rule], common_word: impl Fn(&str) -> Option<bool>) -> Result<Self, Invalid> {
        let mut checked: Vec<Rule> = Vec::new();
        for rule in rules {
            let spelling = rule.spelling.trim();
            if spelling.is_empty() {
                return Err(Invalid::EmptySpelling);
            }
            let trigger = rule.trigger();
            if spelling.chars().any(char::is_control)
                || !trigger.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '\'' | '’' | '-'))
                || !trigger.chars().any(char::is_alphanumeric)
            {
                return Err(Invalid::UnsupportedCharacters);
            }
            if trigger.split_whitespace().count() == 1 {
                // These protected negative fixtures never depend on local dictionary
                // customization or whether a service recognizes a proper name.
                let lower = trigger.to_lowercase();
                if ["will", "mark", "monday", "linear", "may", "bill", "rose", "hope"].contains(&lower.as_str()) {
                    return Err(Invalid::CommonWord);
                }
                match common_word(&lower) {
                    Some(true) => return Err(Invalid::CommonWord),
                    None => return Err(Invalid::SpellingUnavailable),
                    Some(false) => {}
                }
            }
            if let Some(previous) = checked.iter().find(|r| r.trigger().to_lowercase() == trigger.to_lowercase()) {
                if previous.spelling != spelling {
                    return Err(Invalid::ConflictingTrigger);
                }
                continue;
            }
            checked.push(Rule { spelling: spelling.into(), heard_as: trigger.into() });
        }
        checked.sort_by(|a, b| b.trigger().chars().count().cmp(&a.trigger().chars().count())
            .then_with(|| a.trigger().cmp(b.trigger())));
        Ok(Self(checked))
    }

    /// Longest match first, at word boundaries, on original input only. Output is
    /// never fed through the rules again. Protected spans remain byte-identical.
    pub fn apply(&self, text: &str) -> String {
        if self.0.is_empty() {
            return text.to_owned();
        }
        let protected = protected_spans(text);
        let mut out = String::with_capacity(text.len());
        let mut at = 0;
        while at < text.len() {
            let previous = text[..at].chars().next_back();
            let matched = (!previous.is_some_and(word_char)).then(|| {
                self.0.iter().find_map(|rule| {
                    let end = prefix_end(&text[at..], rule.trigger())? + at;
                    if !word_end(&text[end..])
                        || protected.iter().any(|&(start, stop)| at < stop && end > start)
                    {
                        return None;
                    }
                    Some((end, rule))
                })
            }).flatten();
            if let Some((end, rule)) = matched {
                out.push_str(&rule.spelling);
                at = end;
            } else {
                let c = text[at..].chars().next().expect("at is a character boundary before the end");
                out.push(c);
                at += c.len_utf8();
            }
        }
        out
    }
}

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '\'' | '’')
}

fn word_end(suffix: &str) -> bool {
    let mut chars = suffix.chars();
    match chars.next() {
        None => true,
        Some(c) if !word_char(c) => true,
        // Correct the approved name, preserving the possessive on original input.
        // It must itself end at a boundary, never inside an identifier or word.
        Some('\'' | '’') => matches!(chars.next(), Some('s' | 'S'))
            && !chars.next().is_some_and(word_char),
        _ => false,
    }
}

fn prefix_end(text: &str, trigger: &str) -> Option<usize> {
    let mut chars = text.char_indices();
    let mut end = 0;
    for expected in trigger.chars() {
        let (at, actual) = chars.next()?;
        if actual.to_lowercase().ne(expected.to_lowercase()) {
            return None;
        }
        end = at + actual.len_utf8();
    }
    Some(end)
}

/// Deliberately conservative: whitespace-delimited URLs, mail addresses and
/// identifier-like tokens, plus backtick-delimited code (including spaces).
/// An unmatched code delimiter protects the remainder rather than guessing.
fn protected_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut offset = 0;
    for token in text.split_inclusive(char::is_whitespace) {
        let body = token.trim_end();
        let dotted = body.chars().collect::<Vec<_>>().windows(3)
            .any(|w| w[1] == '.' && w[0].is_alphanumeric() && w[2].is_alphanumeric());
        if dotted || body.contains(['@', '_', '/', '\\', '=', '[', ']', '{', '}'])
            || body.contains("::") || body.contains("()")
        {
            spans.push((offset, offset + body.len()));
        }
        offset += token.len();
    }
    let mut cursor = 0;
    while let Some(start) = text[cursor..].find('`').map(|i| cursor + i) {
        let count = text[start..].bytes().take_while(|b| *b == b'`').count();
        let delimiter = &text[start..start + count];
        let after = start + count;
        let end = text[after..].find(delimiter).map(|i| after + i + count).unwrap_or(text.len());
        spans.push((start, end));
        cursor = end;
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(heard: &str, spelling: &str) -> Rule {
        Rule { spelling: spelling.into(), heard_as: heard.into() }
    }

    #[test]
    fn whole_phrases_preserve_punctuation_case_and_ordinary_words() {
        let rules = Rules::checked(&[rule("rich scribe", "RichScribe"), rule("Pria", "Priya")], |_| Some(false)).unwrap();
        assert_eq!(rules.apply("Rich scribe, ask PRIA. A rich scriber and prior notes."),
                   "RichScribe, ask Priya. A rich scriber and prior notes.");
        assert_eq!(rules.apply("pria's x-pria pria_two"), "Priya's x-pria pria_two");
    }

    #[test]
    fn possessive_names_and_phrases_keep_the_suffix_and_protected_boundaries() {
        let rules = Rules::checked(&[rule("Pria", "Priya"), rule("acme corp", "Acme Corp")], |_| Some(false)).unwrap();
        assert_eq!(rules.apply("Send Pria's deck to PRIA’S team. Acme corp's renewal and ACME CORP’s plan."),
                   "Send Priya's deck to Priya’S team. Acme Corp's renewal and Acme Corp’s plan.");
        let unchanged = "x-Pria's Pria'stone Pria’s_team Pria's-team Pria’s’s https://Pria's.com Pria's@example.com `Pria's`";
        assert_eq!(rules.apply(unchanged), unchanged);
    }

    #[test]
    fn longest_original_match_wins_and_output_is_never_reprocessed() {
        let rules = Rules::checked(&[
            rule("rich scribe", "RichScribe"), rule("rich scribe team", "Scribe Team"),
            rule("scribe team", "unwanted chain"),
        ], |_| None).unwrap();
        assert_eq!(rules.apply("Rich scribe team. Rich scribe."), "Scribe Team. RichScribe.");
    }

    #[test]
    fn urls_mail_identifiers_and_code_remain_unchanged_even_across_phrase_boundaries() {
        let rules = Rules::checked(&[rule("Pria", "Priya"), rule("rich scribe", "RichScribe")], |_| Some(false)).unwrap();
        let input = "https://Pria.com Pria@example.com Pria_id Pria() rich scribe@example.com `rich scribe` ```Pria``` (Pria).";
        assert_eq!(rules.apply(input), input.replace("(Pria).", "(Priya)."));
        assert_eq!(rules.apply("`unclosed rich scribe and Pria"), "`unclosed rich scribe and Pria");
    }

    #[test]
    fn common_or_uncertain_single_words_are_refused_but_phrases_work_without_a_service() {
        for word in ["Will", "Mark", "Monday", "Linear"] {
            assert_eq!(Rules::checked(&[rule("", word)], |_| Some(false)).unwrap_err(), Invalid::CommonWord);
        }
        assert_eq!(Rules::checked(&[rule("ship", "Shipp")], |_| Some(true)).unwrap_err(), Invalid::CommonWord);
        assert_eq!(Rules::checked(&[rule("Pria", "Priya")], |_| None).unwrap_err(), Invalid::SpellingUnavailable);
        let phrase = Rules::checked(&[rule("ask will", "ask Will")], |_| None).unwrap();
        assert_eq!(phrase.apply("we will ship. Ask will."), "we will ship. ask Will.");
    }

    #[test]
    fn conflicting_triggers_are_refused_and_identical_rules_are_harmless() {
        assert_eq!(Rules::checked(&[rule("rich scribe", "A"), rule("RICH SCRIBE", "B")], |_| None).unwrap_err(), Invalid::ConflictingTrigger);
        let rules = Rules::checked(&[rule("rich scribe", "A"), rule("RICH SCRIBE", "A")], |_| None).unwrap();
        assert_eq!(rules.apply("Rich scribe"), "A");
        for bad in [rule("name", "\n"), rule("a\nb", "Name"), rule("", "bad\nname")] {
            assert!(Rules::checked(&[bad], |_| Some(false)).is_err());
        }
    }

    #[test]
    fn unicode_and_long_dictation_are_not_truncated() {
        let rules = Rules::checked(&[rule("Māra team", "Māra Team")], |_| None).unwrap();
        let text = "MĀRA TEAM, keep the date. ".repeat(3000);
        let expected = "Māra Team, keep the date. ".repeat(3000);
        assert_eq!(rules.apply(&text), expected);
    }
}
