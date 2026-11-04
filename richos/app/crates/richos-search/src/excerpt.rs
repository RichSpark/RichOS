//! Rich's excerpt: a passage bounded for the tool's answer, keeping the part that matched.
//!
//! A passage can run to 1,200 characters and the tool's evidence item to 600 (`richos-core`'s
//! `search_tools::MAX_EXCERPT_CHARS`). Cutting from the beginning lost an answer that sat late
//! in an ordinary paragraph, even one an exact quoted span had matched. So the bound is placed
//! around the match: the first quoted span the passage holds; else the stretch holding the most
//! of the query's words; else, for a passage matched by meaning alone, the stretch the caller
//! found closest in meaning ([`stretches`] are the candidates). Character offsets throughout.

/// The passage folded as [`crate::text::fold`] folds it, each folded character with the index
/// of the original character it came from, so a match found in the folded text maps back.
fn folded_with_origin(text: &str) -> (Vec<char>, Vec<usize>) {
    let mut chars = Vec::new();
    let mut origin = Vec::new();
    let mut pending_space = None;
    for (i, c) in text.chars().enumerate() {
        if c.is_whitespace() {
            if !chars.is_empty() {
                pending_space = Some(i);
            }
            continue;
        }
        if let Some(at) = pending_space.take() {
            chars.push(' ');
            origin.push(at);
        }
        for lower in c.to_lowercase() {
            chars.push(lower);
            origin.push(i);
        }
    }
    (chars, origin)
}

/// How a needle is matched: exactly as ranking matched it, so the excerpt shows what made the
/// result.
#[derive(Clone, Copy)]
enum Match {
    /// `text::has_span`: the folded span literally, nothing relaxed.
    Span,
    /// `text::has_word`: a whole word, allowing a plural `s` or `es` (`refund` in `refunds`).
    Word,
}

/// Every place `needle` stands in `hay` under `rule`, as folded offsets (a plural ending
/// included in the range).
fn occurrences(hay: &[char], needle: &str, rule: Match) -> Vec<(usize, usize)> {
    let needle: Vec<char> = needle.chars().collect();
    let mut out = Vec::new();
    if needle.is_empty() || needle.len() > hay.len() {
        return out;
    }
    let alnum_at = |i: usize| i < hay.len() && hay[i].is_alphanumeric();
    for start in 0..=hay.len() - needle.len() {
        let mut end = start + needle.len();
        if hay[start..end] != needle[..] {
            continue;
        }
        if let Match::Word = rule {
            if start > 0 && hay[start - 1].is_alphanumeric() {
                continue;
            }
            // text::has_word's order: "es" when nothing alphanumeric follows it, else "s".
            if hay[end..].starts_with(&['e', 's']) && !alnum_at(end + 2) {
                end += 2;
            } else if hay[end..].starts_with(&['s']) {
                end += 1;
            }
            if alnum_at(end) {
                continue;
            }
        }
        out.push((start, end));
    }
    out
}

/// Where in `text` the query matched, as a character range of the original text: the first
/// quoted span it holds; else the stretch of at most `max` characters holding the most distinct
/// query words (the earliest on a tie). `None` when it holds neither: it matched by meaning.
pub fn lexical_focus(text: &str, spans: &[String], words: &[String], max: usize) -> Option<(usize, usize)> {
    let (hay, origin) = folded_with_origin(text);
    let back = |(s, e): (usize, usize)| (origin[s], origin[e - 1] + 1);
    let first_span = spans.iter().filter_map(|span| occurrences(&hay, span, Match::Span).into_iter().next()).min();
    if let Some(found) = first_span {
        return Some(back(found));
    }
    let mut found: Vec<(usize, usize, usize)> = Vec::new();
    for (w, word) in words.iter().enumerate() {
        for (s, e) in occurrences(&hay, word, Match::Word) {
            let (s, e) = back((s, e));
            found.push((s, e, w));
        }
    }
    found.sort();
    let mut best: Option<(usize, (usize, usize))> = None;
    for i in 0..found.len() {
        let mut seen = vec![false; words.len()];
        let mut end = found[i].1;
        for f in &found[i..] {
            if f.1 - found[i].0 > max {
                break;
            }
            seen[f.2] = true;
            end = end.max(f.1);
        }
        let distinct = seen.iter().filter(|s| **s).count();
        if best.is_none_or(|(n, _)| distinct > n) {
            best = Some((distinct, (found[i].0, end)));
        }
    }
    best.map(|(_, range)| range)
}

/// Candidate stretches of at most `max` characters for a passage matched by meaning alone: one
/// starting every half bound, each start moved to the next word, and the last ending at the end.
pub fn stretches(text: &str, max: usize) -> Vec<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    if len <= max {
        return vec![(0, len)];
    }
    let step = (max / 2).max(1);
    let mut out = Vec::new();
    let mut start = 0;
    while start + max < len {
        out.push((start, start + max));
        let mut next = start + step;
        while next < len && !chars[next - 1].is_whitespace() {
            next += 1;
        }
        start = next;
    }
    out.push((len - max, len));
    out
}

/// The characters `range` covers.
pub fn slice(text: &str, (start, end): (usize, usize)) -> String {
    text.chars().skip(start).take(end - start).collect()
}

/// `text` bounded to `max` characters around `focus`: the whole of it when it fits; otherwise
/// a window holding the focus (centered when the focus is shorter than the bound, starting at
/// it when longer), moved off a word it would cut where that keeps the focus whole, with "…"
/// where text was left out. No focus keeps the beginning.
pub fn excerpt(text: &str, focus: Option<(usize, usize)>, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    if len <= max {
        return text.to_string();
    }
    let (fs, fe) = focus.unwrap_or((0, 0));
    let mut start = if fe - fs >= max { fs } else { fs.saturating_sub((max - (fe - fs)) / 2) };
    start = start.min(len - max);
    let mut end = start + max;
    // Off a cut word, by at most a word's length, and never into the focus.
    const WORD: usize = 40;
    if start > 0 && start < fs {
        if let Some(k) = (start..=fs.min(start + WORD)).find(|&k| chars[k - 1].is_whitespace()) {
            start = k;
        }
    }
    if end < len && end > fe {
        if let Some(k) = (fe.max(end.saturating_sub(WORD))..=end).rev().find(|&k| chars[k].is_whitespace()) {
            end = k;
        }
    }
    let body: String = chars[start..end].iter().collect::<String>().trim().to_string();
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.push_str(&body);
    if end < len {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn a_short_passage_is_kept_whole() {
        assert_eq!(excerpt("short", Some((0, 5)), 600), "short");
        assert_eq!(excerpt("short", None, 600), "short");
    }

    #[test]
    fn with_no_focus_the_beginning_is_kept_as_before() {
        let text = "x".repeat(700);
        assert_eq!(excerpt(&text, None, 600), "x".repeat(600) + "…");
    }

    #[test]
    fn a_quoted_span_late_in_a_long_paragraph_stays_in_the_excerpt() {
        let text = "Filler words here. ".repeat(40) + "We chose the  Two-Week onboarding call in the end.";
        let focus = lexical_focus(&text, &words(&["two-week onboarding call"]), &words(&["onboarding"]), 600).unwrap();
        assert_eq!(slice(&text, focus), "Two-Week onboarding call");
        let out = excerpt(&text, Some(focus), 600);
        assert!(out.contains("Two-Week onboarding call in the end."), "{out}");
        assert!(out.starts_with('…') && out.chars().count() <= 602, "{out}");
    }

    #[test]
    fn matches_follow_the_ranking_rules_spans_literal_words_whole_with_plurals() {
        // text::has_span is literal, so a span is found where ranking found it.
        assert_eq!(lexical_focus("the callback ran", &words(&["call"]), &[], 600), Some((4, 8)));
        // text::has_word: whole words only, with a plural "s" or "es" included in the range.
        assert_eq!(lexical_focus("the callback ran", &[], &words(&["call"]), 600), None);
        let text = "we give refunds";
        assert_eq!(lexical_focus(text, &[], &words(&["refund"]), 600).map(|r| slice(text, r)).as_deref(), Some("refunds"));
        let text = "two boxes";
        assert_eq!(lexical_focus(text, &[], &words(&["box"]), 600).map(|r| slice(text, r)).as_deref(), Some("boxes"));
        assert_eq!(lexical_focus("dayss", &[], &words(&["day"]), 600), None);
        assert_eq!(lexical_focus("today", &[], &words(&["day"]), 600), None);
    }

    #[test]
    fn the_stretch_with_the_most_query_words_wins() {
        let text = "pricing ".to_string() + &"filler ".repeat(120) + "pricing for the coach tier";
        let focus = lexical_focus(&text, &[], &words(&["pricing", "coach", "tier"]), 600).unwrap();
        assert_eq!(slice(&text, focus), "pricing for the coach tier");
    }

    #[test]
    fn stretches_cover_the_whole_passage_and_each_stays_bounded() {
        let text = "word ".repeat(250);
        let all = stretches(&text, 600);
        assert_eq!(all.first().unwrap().0, 0);
        assert_eq!(all.last().unwrap().1, text.chars().count());
        assert!(all.iter().all(|(s, e)| e - s <= 600));
        assert_eq!(stretches("short", 600), vec![(0, 5)]);
    }
}
