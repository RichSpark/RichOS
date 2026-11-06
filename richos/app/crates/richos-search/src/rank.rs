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
    let mut out: Vec<_> = corpus.docs.iter().enumerate()
        .filter_map(|(i, doc)| lexical_hit(i, doc, query, strict)).collect();
    sort_lexical(corpus, &mut out);
    out
}

fn lexical_hit(i: usize, doc: &Document, query: &ParsedQuery, strict: bool) -> Option<LexicalHit> {
    let spans = spans_in(doc, &query.spans);
    if strict && spans < query.spans.len() {
        return None;
    }
    let title_words = count_words(&doc.folded_title, &query.words);
    let file_words = query.words.iter().filter(|w| has_word(&doc.folded_title, w) || has_word(&doc.folded_text, w)).count();
    (file_words > 0 || spans > 0).then_some(LexicalHit { doc: i, title_words, spans, file_words })
}

fn sort_lexical(corpus: &Corpus, hits: &mut [LexicalHit]) {
    hits.sort_by(|a, b| {
        b.title_words
            .cmp(&a.title_words)
            .then(b.spans.cmp(&a.spans))
            .then(b.file_words.cmp(&a.file_words))
            .then_with(|| by_date_then_path(&corpus.docs[a.doc], &corpus.docs[b.doc]))
    });
}

#[derive(Default)]
struct WordHit {
    doc: usize,
    title: bool,
    body: bool,
}

/// Inverted postings for the word rules. A quoted span can match inside a
/// token or across whitespace, so those queries still use the literal scan.
pub(crate) struct LexicalIndex {
    words: HashMap<String, Vec<WordHit>>,
}

/// Markdown remains in its original order when media is inserted or removed.
pub(crate) struct UpdatedPositions {
    markdown: Vec<usize>,
    media: Vec<usize>,
}

impl UpdatedPositions {
    pub(crate) fn for_corpus(corpus: &Corpus) -> Self {
        let mut result = Self { markdown: Vec::new(), media: Vec::new() };
        for (i, doc) in corpus.docs.iter().enumerate() {
            if doc.metadata_version.is_some() {
                result.media.push(i);
            } else {
                result.markdown.push(i);
            }
        }
        result
    }
}

impl LexicalIndex {
    pub(crate) fn build(corpus: &Corpus, stopped: impl Fn() -> bool) -> Option<Self> {
        let mut words: HashMap<String, Vec<WordHit>> = HashMap::new();
        for (doc, source) in corpus.docs.iter().enumerate() {
            if stopped() {
                return None;
            }
            for (is_title, text) in [(true, &source.folded_title), (false, &source.folded_text)] {
                for word in text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
                    let hits = match words.get_mut(word) {
                        Some(hits) => hits,
                        None => words.entry(word.to_owned()).or_default(),
                    };
                    if hits.last().is_none_or(|hit| hit.doc != doc) {
                        hits.push(WordHit { doc, ..WordHit::default() });
                    }
                    let hit = hits.last_mut().expect("the current document has a posting");
                    if is_title {
                        hit.title = true;
                    } else {
                        hit.body = true;
                    }
                }
            }
        }
        Some(Self { words })
    }

    pub(crate) fn search(&self, corpus: &Corpus, query: &ParsedQuery, strict: bool, positions: Option<&UpdatedPositions>) -> Vec<LexicalHit> {
        if !query.spans.is_empty() {
            return lexical(corpus, query, strict);
        }
        #[derive(Clone, Default)]
        struct Counts {
            title_words: usize,
            file_words: usize,
            last_title: usize,
            last_file: usize,
        }
        let mut counts = vec![Counts::default(); corpus.docs.len()];
        let position = |doc| positions.map_or(doc, |p| p.markdown[doc]);
        for (i, word) in query.words.iter().enumerate() {
            let mark = i + 1;
            let mut add = |doc: usize, title: bool, body: bool| {
                let c = &mut counts[doc];
                if title && c.last_title != mark {
                    c.last_title = mark;
                    c.title_words += 1;
                }
                if (title || body) && c.last_file != mark {
                    c.last_file = mark;
                    c.file_words += 1;
                }
            };
            if word.chars().all(char::is_alphanumeric) {
                for key in [word.clone(), format!("{word}s"), format!("{word}es")] {
                    if let Some(hits) = self.words.get(&key) {
                        for h in hits {
                            add(position(h.doc), h.title, h.body);
                        }
                    }
                }
            } else {
                // For an identifier such as inv-2049, only files with the
                // opening token can match. Verify the punctuation in those.
                let anchor = word.split(|c: char| !c.is_alphanumeric()).next().unwrap_or_default();
                if let Some(hits) = self.words.get(anchor) {
                    for h in hits {
                        let doc = position(h.doc);
                        let d = &corpus.docs[doc];
                        add(doc, h.title && has_word(&d.folded_title, word), h.body && has_word(&d.folded_text, word));
                    }
                }
            }
        }
        let mut hits: Vec<_> = counts.into_iter().enumerate().filter_map(|(doc, c)| {
            (c.file_words > 0).then_some(LexicalHit { doc, title_words: c.title_words, spans: 0, file_words: c.file_words })
        }).collect();
        if let Some(positions) = positions {
            hits.extend(positions.media.iter().filter_map(|&i| lexical_hit(i, &corpus.docs[i], query, strict)));
        }
        sort_lexical(corpus, &mut hits);
        hits
    }
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
/// 3. Otherwise, the PRD's text rule: the passage with the most query-word matches; failing
///    any match, the first.
///
/// Rule 1 is a tier, not a weight: only the passages holding the most quoted spans are
/// candidates, and words decide among them. As a weight (a span worth five, a word two) an
/// agenda paragraph with five of the query's words beat the decision holding the quoted text
/// (fourth review: "annual plan pricing support cost \"30 days\"" showed the agenda in all
/// four modes, to the palette and to Rich).
pub fn best_passage(doc: &Document, query: &ParsedQuery, semantic_passage: Option<usize>) -> usize {
    let semantic = semantic_passage.filter(|p| *p < doc.passages.len());
    // A span counts when one segment (a heading or the passage text) holds it whole, or when it
    // runs from the passage into the next one ([`crosses_into_next`]).
    let spans_in = |segments: &[String]| query.spans.iter().filter(|s| segments.iter().any(|f| has_span(f, s))).count();
    let words_in = |segments: &[String]| query.words.iter().filter(|w| segments.iter().any(|f| has_word(f, w))).count();
    let loose = loose_spans(doc, query);
    let spans: Vec<usize> = doc.folded_passages.iter().enumerate().map(|(i, f)| spans_in(f) + across(doc, &loose, i)).collect();
    let most_spans = spans.iter().copied().max().unwrap_or(0);
    if let (Some(p), 0) = (semantic, most_spans) {
        return p;
    }
    let mut best: Option<(usize, usize)> = None;
    for (i, folded) in doc.folded_passages.iter().enumerate() {
        if spans[i] < most_spans {
            continue;
        }
        let score = words_in(folded);
        let better = match best {
            None => true,
            Some((_, b)) => score > b || (score == b && semantic == Some(i)),
        };
        if better {
            best = Some((i, score));
        }
    }
    match best {
        Some((i, score)) if score > 0 || most_spans > 0 => i,
        _ => semantic.unwrap_or(0),
    }
}

/// Rerank the passages of an already selected file. Each distinct term in the body
/// contributes once, with more weight for a term found in fewer of this file's
/// passages. The bounded word bonus lets a closer textual answer win a near tie
/// without requiring literal overlap for a paraphrase.
pub fn best_passage_with_scores(
    doc: &Document,
    query: &ParsedQuery,
    semantic_passage: Option<usize>,
    scores: Option<&[f32]>,
) -> usize {
    let Some(scores) = scores.filter(|s| s.len() == doc.passages.len() && s.len() > 1) else {
        return best_passage(doc, query, semantic_passage);
    };
    if !query.spans.is_empty() {
        return best_passage(doc, query, semantic_passage);
    }

    const MAX_WORD_BONUS: f32 = 0.16;
    let mut matches = vec![0.0; scores.len()];
    let mut possible = 0.0;
    for word in &query.words {
        let matching: Vec<_> = doc.folded_passages.iter().enumerate()
            .filter(|(_, parts)| parts.last().is_some_and(|body| has_word(body, word)))
            .map(|(i, _)| i)
            .collect();
        if matching.is_empty() {
            continue;
        }
        let weight = 1.0 + ((scores.len() + 1) as f32 / (matching.len() + 1) as f32).ln();
        possible += weight;
        for i in matching {
            matches[i] += weight;
        }
    }

    let score = |i: usize| scores[i] + MAX_WORD_BONUS * matches[i] / possible.max(1.0);
    (0..scores.len()).max_by(|&a, &b| {
        score(a).total_cmp(&score(b))
            .then_with(|| scores[a].total_cmp(&scores[b]))
            .then_with(|| b.cmp(&a))
    }).unwrap_or(0)
}

/// The query's quoted spans the document holds that no single passage holds whole: a quote
/// across the cut of a paragraph longer than [`crate::chunk::MAX_CHARS`]. Strict search finds it
/// in the document, so the passage it begins in is the evidence (rv-20261014T132419Z-a81de6d6-6891
/// #3: an unrelated agenda, or half the sentence, was shown instead).
fn loose_spans<'q>(doc: &Document, query: &'q ParsedQuery) -> Vec<&'q String> {
    query
        .spans
        .iter()
        .filter(|s| has_span(&doc.folded_text, s) && !doc.folded_passages.iter().any(|f| f.iter().any(|seg| has_span(seg, s))))
        .collect()
}

/// How many of `loose` run from passage `i` into the next one.
fn across(doc: &Document, loose: &[&String], i: usize) -> usize {
    let (Some(a), Some(b)) = (doc.passages.get(i), doc.passages.get(i + 1)) else { return 0 };
    if loose.is_empty() {
        return 0;
    }
    let joined = crate::text::fold(&doc.text[a.start..b.end]);
    loose.iter().filter(|s| has_span(&joined, s)).count()
}

/// A quoted span of the query runs from passage `p` into the next one, so what is shown for `p`
/// must take in the start of the next passage to deliver the quote whole.
pub fn crosses_into_next(doc: &Document, query: &ParsedQuery, p: usize) -> bool {
    across(doc, &loose_spans(doc, query), p) > 0
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
        Corpus { docs: docs.into_iter().map(std::sync::Arc::new).collect(), ..Corpus::default() }
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
    fn indexed_word_retrieval_matches_the_original_literal_rules() {
        let c = corpus(vec![
            doc("a.md", "# Names, INV-2049 and prices\nDmitri’s estimate. Caches and API.v2.", 3),
            doc("b.md", "# Price list\nInv-2049s. Cached result and APIs. Dmitri's proposal.", 4),
            doc("c.md", "# Cache API\nINV 2049 and DMITRI. Price changes inside the building.", 4),
            doc("d.md", "# ÄRGER\nLots of lowercase ärgers. Prefixterminal is one word.", 6),
        ]);
        let index = LexicalIndex::build(&c, || false).unwrap();
        for text in ["names inv price", "dmitri's", "dmitri’s", "cache api.v2", "inv-2049", "ärger", "price cache", "\"terminal\"", "\"inv 2049\"", "is in"] {
            let q = parse(text);
            for strict in [false, true] {
                assert_eq!(index.search(&c, &q, strict, None), lexical(&c, &q, strict), "{text}, strict={strict}");
            }
        }
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

    #[test]
    fn passage_reranking_uses_evidence_in_the_body_and_preserves_literal_quotes() {
        let d = doc("a.md", "# Planning\nDiscussion of profile location.\n\n## Decision\nUse a directory junction to keep the profile on another drive.\n\n## Note\nKeep backups.", 0);
        let q = parse("profile location on another drive");
        assert_eq!(best_passage_with_scores(&d, &q, Some(0), Some(&[0.65, 0.64, 0.1])), 1);
        assert_eq!(best_passage_with_scores(&d, &parse("recover my account"), Some(0), Some(&[0.5, 0.7, 0.1])), 1);
        assert_eq!(best_passage_with_scores(&d, &parse("\"keep backups\""), Some(0), Some(&[0.99, 0.8, 0.1])), 2);
    }

    fn shown(d: &Document, query: &str) -> String {
        let p = &d.passages[best_passage(d, &parse(query), None)];
        p.text(&d.text).to_string()
    }

    /// Review rv-20261013T151528Z-71779eae-6117, finding 1: `## Annual plan` over
    /// `### Pricing` is not the phrase "annual plan pricing". Only the decision holds it; the
    /// agenda under the two headings must not count it and win on the other words.
    #[test]
    fn a_quoted_span_never_spans_two_headings() {
        let text = "# Review\n\n## Annual plan\n\n### Pricing\n\nSupport cost is on tomorrow's agenda. No decision has been made.\n\n## Decision\n\nWe approved annual plan pricing at $120 per customer.\n";
        let d = doc("pricing.md", text, 0);
        let q = parse("\"annual plan pricing\" support cost");
        assert!(eligible(&d, &q, true));
        assert!(shown(&d, "\"annual plan pricing\" support cost").contains("$120"));
    }

    /// Review rv-20261013T151528Z-71779eae-6117, finding 2: a Setext heading (`Support cost`
    /// over a `---` line) ends the `## Refunds within 30 days` section, so the agenda under it
    /// no longer carries that heading's quote and the decision holding it is shown.
    #[test]
    fn a_setext_heading_closes_the_heading_path_above_it() {
        let text = format!(
            "# Review\n\n## Refunds within 30 days\n\n### Final decision\n\nThis is the final decision for every new customer. {}\n\nSupport cost\n------------\n\nAnnual plan pricing support cost. This is unrelated agenda text.\n",
            "The implementation remains unchanged. ".repeat(17)
        );
        let d = doc("pricing.md", &text, 0);
        let query = "annual plan pricing support cost \"30 days\"";
        assert!(eligible(&d, &parse(query), true));
        assert!(shown(&d, query).contains("final decision for every new customer"));
    }

    /// Review rv-20261014T102118Z-935c9853-733c: a paragraph and a list over a `---` divider
    /// is not a Setext heading. The list stays passage text, and the agenda below the divider
    /// does not inherit "30 days" from it and win over the decision.
    #[test]
    fn a_list_over_a_divider_stays_text_and_the_agenda_below_does_not_win() {
        let text = "# Pricing review\n\n## Decision\n\nAnnual plan pricing is fixed at $120 per customer, with refunds within 30 days.\n\n## Discussion notes\n\nItems to revisit:\n- Check staffing within 30 days.\n---\n\nAnnual plan pricing support cost is on tomorrow's agenda. No decision has been made about staffing. The committee will ask the operations team to bring a breakdown of workload by region and by shift. Each manager should distinguish routine questions from problems that need an engineer. The discussion will cover training materials, response times, escalation routes and the schedule for the next hiring round. Those topics remain open until the team has gathered the missing information. The finance team will prepare a separate worksheet showing the assumptions behind its estimates. The worksheet should include the expected volume of customer requests and the time needed to answer each type. Nobody should treat these preliminary estimates as a new policy or a change to the approved customer terms.\n";
        let d = doc("pricing.md", text, 0);
        let query = "annual plan pricing support cost \"30 days\"";
        assert!(d.passages.iter().all(|p| !p.heading.starts_with("Items to revisit")));
        assert!(d.passages.iter().any(|p| p.text(&d.text).contains("Items to revisit:\n- Check staffing within 30 days.")));
        for semantic in [None, Some(0)] {
            let p = &d.passages[best_passage(&d, &parse(query), semantic)];
            assert!(p.text(&d.text).contains("$120"), "semantic {semantic:?} chose {:?}", p.heading);
        }
    }
}
