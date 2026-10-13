//! The one text list both modes share, and Super Finder's fusion of it with meaning matches.
//!
//! Search PRD v3.1, "Two independent checkboxes" and "A comparison the CEO can trust":
//!
//! - **Text candidates**: *"A text candidate contains at least one query word or complete
//!   quoted span in its title or body."*
//! - **Text order**: *"Order text candidates by the number of distinct query words in the
//!   title, then literal quoted spans matched, then distinct query words in the file, then file
//!   modification time descending and finally path ascending."*
//! - **Strict**: every quoted span must be in the document (title or text), applied to both
//!   lists before fusion.
//! - **Off**: *"Off uses the lexical list above."* **On**: *"fuses that same list with Qdrant's
//!   ranked semantic candidates using reciprocal rank fusion with constant 60, counting each
//!   source once per list."*
//! - **Final literal boost, both modes**: *"sources containing more complete quoted spans rank
//!   first, followed by lexical order when off or fused score when on. Resolve remaining ties
//!   by modification time and path."*
//!
//! Raw vector scores are never compared with text scores; only ranks are fused.
use crate::corpus::{Corpus, Document};
use crate::query::ParsedQuery;
use crate::text::{count_words, has_span, has_word};
use std::cmp::Ordering;
use std::collections::HashMap;

/// Reciprocal rank fusion's constant, the PRD's number.
pub const RRF_K: f64 = 60.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LexicalHit {
    pub doc: usize,
    pub title_words: usize,
    pub spans: usize,
    pub file_words: usize,
}

fn spans_in(doc: &Document, spans: &[String]) -> usize {
    spans.iter().filter(|s| has_span(&doc.folded_title, s) || has_span(&doc.folded_text, s)).count()
}

/// Does the document satisfy Require quoted text? With no quoted spans it has no effect.
pub fn eligible(doc: &Document, query: &ParsedQuery, strict: bool) -> bool {
    !strict || spans_in(doc, &query.spans) == query.spans.len()
}

fn by_date_then_path(a: &Document, b: &Document) -> Ordering {
    b.modified_ms.cmp(&a.modified_ms).then_with(|| a.rel_path.cmp(&b.rel_path))
}

/// The text list: one scan over the cached text of every document.
pub fn lexical(corpus: &Corpus, query: &ParsedQuery, strict: bool) -> Vec<LexicalHit> {
    let mut out: Vec<LexicalHit> = Vec::new();
    for (i, doc) in corpus.docs.iter().enumerate() {
        let spans = spans_in(doc, &query.spans);
        if strict && spans < query.spans.len() {
            continue;
        }
        let title_words = count_words(&doc.folded_title, &query.words);
        let file_words = query.words.iter().filter(|w| has_word(&doc.folded_title, w) || has_word(&doc.folded_text, w)).count();
        if file_words == 0 && spans == 0 {
            continue;
        }
        out.push(LexicalHit { doc: i, title_words, spans, file_words });
    }
    out.sort_by(|a, b| {
        b.title_words
            .cmp(&a.title_words)
            .then(b.spans.cmp(&a.spans))
            .then(b.file_words.cmp(&a.file_words))
            .then_with(|| by_date_then_path(&corpus.docs[a.doc], &corpus.docs[b.doc]))
    });
    out
}

/// Why a result is in the list, shown on its row (design round 26: *meaning*, *words*,
/// *meaning · words*, *exact "…"*).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Why {
    Exact,
    Both,
    Meaning,
    Words,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ranked {
    pub doc: usize,
    pub why: Why,
    /// Complete quoted spans the document contains.
    pub spans: usize,
    pub lexical_rank: Option<usize>,
    pub semantic_rank: Option<usize>,
    pub fused: f64,
    /// The passage the meaning search matched best, when it matched this document.
    pub semantic_passage: Option<usize>,
}

/// One meaning candidate: a document and its best-matching passage, best first. The caller
/// has already reduced passage hits to one entry per source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemanticHit {
    pub doc: usize,
    pub passage: usize,
}

/// The final order. `semantic` is `None` when Super Finder is off (or unavailable): the list
/// is then the text list with the literal boost, and nothing about meaning is consulted.
pub fn rank(corpus: &Corpus, query: &ParsedQuery, strict: bool, lexical: &[LexicalHit], semantic: Option<&[SemanticHit]>) -> Vec<Ranked> {
    let mut rows: Vec<Ranked> = Vec::new();
    let mut at: HashMap<usize, usize> = HashMap::new();
    for (i, hit) in lexical.iter().enumerate() {
        at.insert(hit.doc, rows.len());
        rows.push(Ranked {
            doc: hit.doc,
            why: Why::Words,
            spans: hit.spans,
            lexical_rank: Some(i + 1),
            semantic_rank: None,
            fused: 1.0 / (RRF_K + (i + 1) as f64),
            semantic_passage: None,
        });
    }
    if let Some(semantic) = semantic {
        let mut rank = 0usize;
        for hit in semantic {
            let doc = &corpus.docs[hit.doc];
            if !eligible(doc, query, strict) {
                continue;
            }
            // Once per source per list: a second passage of the same document adds nothing.
            if at.get(&hit.doc).is_some_and(|&r| rows[r].semantic_rank.is_some()) {
                continue;
            }
            rank += 1;
            let share = 1.0 / (RRF_K + rank as f64);
            match at.get(&hit.doc) {
                Some(&r) => {
                    rows[r].semantic_rank = Some(rank);
                    rows[r].fused += share;
                    rows[r].semantic_passage = Some(hit.passage);
                }
                None => {
                    at.insert(hit.doc, rows.len());
                    rows.push(Ranked {
                        doc: hit.doc,
                        why: Why::Meaning,
                        spans: spans_in(doc, &query.spans),
                        lexical_rank: None,
                        semantic_rank: Some(rank),
                        fused: share,
                        semantic_passage: Some(hit.passage),
                    });
                }
            }
        }
    }
    let on = semantic.is_some();
    rows.sort_by(|a, b| {
        let first = b.spans.cmp(&a.spans);
        let second = if on {
            b.fused.partial_cmp(&a.fused).unwrap_or(Ordering::Equal)
        } else {
            a.lexical_rank.cmp(&b.lexical_rank)
        };
        first.then(second).then_with(|| by_date_then_path(&corpus.docs[a.doc], &corpus.docs[b.doc]))
    });
    for row in &mut rows {
        row.why = if row.spans > 0 {
            Why::Exact
        } else {
            match (row.lexical_rank, row.semantic_rank) {
                (Some(_), Some(_)) => Why::Both,
                (None, Some(_)) => Why::Meaning,
                _ => Why::Words,
            }
        };
    }
    rows
}

/// The passage shown under a result.
///
/// 1. A passage holding a quoted span always wins: the CEO asked for that text.
/// 2. Otherwise, when meaning search matched this document (Super Finder on), the passage it
///    matched: that is the passage the meaning was found in. Found on the harness's first run
///    over the stand-in folder: for "which subject line won the welcome emails" the
///    word-count rule picked one of sixty early brainstorm passages full of "welcome emails"
///    and "subject lines" and never the late passage that answers the question.
/// 3. Otherwise, the PRD's text rule: the passage with the most query-word matches (a quoted
///    span counts more than a word, the design's weighting); failing any match, the first.
pub fn best_passage(doc: &Document, query: &ParsedQuery, semantic_passage: Option<usize>) -> usize {
    let semantic = semantic_passage.filter(|p| *p < doc.passages.len());
    let span_passage = doc.folded_passages.iter().any(|f| query.spans.iter().any(|s| has_span(f, s)));
    if let (Some(p), false) = (semantic, span_passage) {
        return p;
    }
    let mut best: Option<(usize, usize)> = None;
    for (i, folded) in doc.folded_passages.iter().enumerate() {
        let score = count_words(folded, &query.words) * 2 + query.spans.iter().filter(|s| has_span(folded, s)).count() * 5;
        let better = match best {
            None => true,
            Some((_, b)) => score > b || (score == b && semantic_passage == Some(i)),
        };
        if better {
            best = Some((i, score));
        }
    }
    match best {
        Some((i, score)) if score > 0 => i,
        _ => semantic.unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::Document;
    use crate::query::parse;

    fn doc(path: &str, text: &str, modified_ms: u64) -> Document {
        Document::from_text(path, text.to_string(), modified_ms, text.len() as u64, String::new())
    }

    fn corpus(docs: Vec<Document>) -> Corpus {
        Corpus { docs, ..Corpus::default() }
    }

    fn order(c: &Corpus, ranked: &[Ranked]) -> Vec<String> {
        ranked.iter().map(|r| c.docs[r.doc].rel_path.clone()).collect()
    }

    #[test]
    fn text_order_is_title_words_then_spans_then_file_words_then_date_then_path() {
        let c = corpus(vec![
            doc("a.md", "# Notes\nrefund policy and pricing", 1),
            doc("b.md", "# Refund policy\nnothing else", 1),
            doc("c.md", "# Other\nthe refund", 5),
            doc("d.md", "# Other\nthe refund", 5),
            doc("e.md", "# Unrelated\nnothing to see", 9),
        ]);
        let q = parse("refund policy pricing");
        let lex = lexical(&c, &q, false);
        let ranked = rank(&c, &q, false, &lex, None);
        assert_eq!(order(&c, &ranked), vec!["b.md", "a.md", "c.md", "d.md"]);
        assert!(ranked.iter().all(|r| r.why == Why::Words));
    }

    #[test]
    fn strict_requires_every_span_across_passages_and_off_boosts_spans_first() {
        let c = corpus(vec![
            doc("exact.md", "# Cancel\nCancel within 30 days.", 1),
            doc("thirty.md", "# Cancel and cancel\nCancel within thirty days.", 9),
            doc("both.md", "# x\nOur pricing.\n\n## Later\n\nThe support cost is high.", 1),
            doc("one.md", "# x\nOur pricing only.", 1),
        ]);
        let strict = rank(&c, &parse("cancel \"30 days\""), true, &lexical(&c, &parse("cancel \"30 days\""), true), None);
        assert_eq!(order(&c, &strict), vec!["exact.md"]);
        assert_eq!(strict[0].why, Why::Exact);
        let q = parse("cancel \"30 days\"");
        let loose = rank(&c, &q, false, &lexical(&c, &q, false), None);
        assert_eq!(order(&c, &loose), vec!["exact.md", "thirty.md"], "the exact one stays first in both modes");
        let two = parse("\"pricing\" \"support cost\"");
        assert_eq!(order(&c, &rank(&c, &two, true, &lexical(&c, &two, true), None)), vec!["both.md"]);
        assert_eq!(order(&c, &rank(&c, &parse("\"INV-2049\""), true, &lexical(&c, &parse("\"INV-2049\""), true), None)), Vec::<String>::new());
    }

    #[test]
    fn no_quotes_means_strict_has_no_effect() {
        let c = corpus(vec![doc("a.md", "refund", 1)]);
        let q = parse("refund");
        assert_eq!(lexical(&c, &q, true), lexical(&c, &q, false));
    }

    #[test]
    fn super_finder_fuses_by_rank_counting_each_source_once_and_finds_a_document_without_the_words() {
        let c = corpus(vec![
            doc("words.md", "# Slow app\nthe app was slow", 1),
            doc("meaning.md", "# Why the iPhone build takes three seconds\nlaunch time", 1),
            doc("both.md", "# App startup\nslow", 1),
        ]);
        let q = parse("the app felt slow on first open");
        let lex = lexical(&c, &q, false);
        assert!(lex.iter().all(|h| c.docs[h.doc].rel_path != "meaning.md"), "text search cannot find it");
        let sem = [SemanticHit { doc: 1, passage: 0 }, SemanticHit { doc: 1, passage: 0 }, SemanticHit { doc: 2, passage: 0 }];
        let ranked = rank(&c, &q, false, &lex, Some(&sem));
        let meaning = ranked.iter().find(|r| c.docs[r.doc].rel_path == "meaning.md").unwrap();
        assert_eq!(meaning.why, Why::Meaning);
        assert_eq!(meaning.semantic_rank, Some(1));
        let both = ranked.iter().find(|r| c.docs[r.doc].rel_path == "both.md").unwrap();
        assert_eq!(both.why, Why::Both);
        assert_eq!(both.semantic_rank, Some(2), "the duplicate passage of meaning.md did not take a rank");
        assert!((both.fused - (1.0 / 62.0 + 1.0 / (60.0 + both.lexical_rank.unwrap() as f64))).abs() < 1e-12);
        assert_eq!(c.docs[ranked[0].doc].rel_path, "both.md", "in both lists beats either alone");
    }

    #[test]
    fn strict_filters_the_meaning_list_too_and_a_vector_cutoff_cannot_hide_an_exact_match() {
        let c = corpus(vec![doc("exact.md", "fee of \"INV-2049\" INV-2049", 1), doc("near.md", "an invoice number", 1)]);
        let q = parse("\"INV-2049\"");
        // Meaning search returned only the near miss; the exact document is still found by text.
        let ranked = rank(&c, &q, true, &lexical(&c, &q, true), Some(&[SemanticHit { doc: 1, passage: 0 }]));
        assert_eq!(order(&c, &ranked), vec!["exact.md"]);
    }

    #[test]
    fn the_excerpt_is_the_passage_with_most_matches_or_the_meaning_passage() {
        let d = doc("a.md", "# A\nnothing here\n\n## B\n\nrefund within 30 days\n\n## C\n\nthe launch takes three seconds", 1);
        assert_eq!(best_passage(&d, &parse("refund days"), None), 1);
        assert_eq!(best_passage(&d, &parse("slow first open"), Some(2)), 2);
        assert_eq!(best_passage(&d, &parse("slow first open"), None), 0);
        // Meaning matched passage 2: it is shown even though passage 1 holds more of the words.
        assert_eq!(best_passage(&d, &parse("refund days"), Some(2)), 2);
        // A quoted span outranks the meaning passage.
        assert_eq!(best_passage(&d, &parse("\"30 days\""), Some(2)), 1);
    }
}
