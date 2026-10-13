//! Splitting one Markdown file into short passages, each with its place in the file.
//!
//! Search PRD v3.1: *"Split by headings and paragraphs into short passages suitable for the
//! chosen embedder, retaining source offsets. Cover the full supported file, including late
//! passages. … Do not treat an entire long chat as one vector."*
//!
//! The rule, in order:
//! 1. A heading line (`#` to `######`) closes the passage being built and becomes the heading
//!    of the passages after it.
//! 2. Blank lines separate paragraphs. A fenced code block (```` ``` ```` or `~~~`) is one
//!    paragraph however many blank lines it holds.
//! 3. Paragraphs join into one passage until the next one would take it past
//!    [`TARGET_CHARS`].
//! 4. A single paragraph longer than [`MAX_CHARS`] is cut at sentence ends (`. ! ?` followed by
//!    a space), or at a line break or space when a sentence is longer still, so no passage is
//!    longer than [`MAX_CHARS`].
//!
//! Every byte of the file outside heading lines and blank lines belongs to exactly one passage,
//! so a late answer in a long chat is as findable as an early one. Offsets are byte offsets into
//! the original UTF-8 text and always fall on character boundaries.

/// The size a passage grows to before a new one starts. Short enough for a sentence-embedding
/// model to represent its subject, long enough to show as a useful excerpt. Recorded in the
/// index identity ([`CHUNKER_IDENTITY`]), so changing it re-embeds.
pub const TARGET_CHARS: usize = 700;
/// No passage is longer than this.
pub const MAX_CHARS: usize = 1200;
/// Changes whenever the rule above or its numbers change; part of every index's identity.
pub const CHUNKER_IDENTITY: &str = "md-paragraphs-v1-700-1200";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Passage {
    /// The nearest heading above the passage, without its `#` marks; empty before any heading.
    pub heading: String,
    /// Byte range of the passage in the file.
    pub start: usize,
    pub end: usize,
    /// 1-based line of the passage's first character.
    pub line: u32,
}

impl Passage {
    pub fn text<'a>(&self, file: &'a str) -> &'a str {
        &file[self.start..self.end]
    }
}

fn heading_of(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) {
        let rest = &trimmed[hashes..];
        if rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t') {
            return Some(rest.trim().trim_end_matches('#').trim());
        }
    }
    None
}

fn is_fence(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("```") || t.starts_with("~~~")
}

/// One block of the file: a run of non-blank lines (or one fenced code block), as a byte range.
struct Block {
    start: usize,
    end: usize,
    line: u32,
}

/// Cut `start..end` of `text` into pieces of at most `MAX_CHARS` characters, preferring
/// sentence ends, then line breaks, then spaces, and as a last resort a character boundary.
fn split_long(text: &str, start: usize, end: usize, out: &mut Vec<(usize, usize)>) {
    let mut from = start;
    while text[from..end].chars().count() > MAX_CHARS {
        // The byte offset just past MAX_CHARS characters.
        let limit = from + text[from..end].char_indices().nth(MAX_CHARS).map(|(i, _)| i).unwrap_or(end - from);
        let window = &text[from..limit];
        let floor = window.char_indices().nth(MAX_CHARS / 3).map(|(i, _)| i).unwrap_or(0);
        let cut = [". ", "! ", "? ", "\n", " "]
            .iter()
            .find_map(|sep| window.rfind(sep).filter(|at| *at >= floor).map(|at| at + sep.len()))
            .unwrap_or(window.len());
        out.push((from, from + cut));
        from += cut;
        while from < end && text[from..].starts_with(char::is_whitespace) {
            from += text[from..].chars().next().map(char::len_utf8).unwrap_or(1);
        }
    }
    if from < end {
        out.push((from, end));
    }
}

/// The passages of one Markdown file, in file order.
pub fn passages(text: &str) -> Vec<Passage> {
    let mut out: Vec<Passage> = Vec::new();
    let mut heading = String::new();
    let mut blocks: Vec<Block> = Vec::new();

    // Close the blocks gathered under one heading into passages.
    fn flush(text: &str, heading: &str, blocks: &mut Vec<Block>, out: &mut Vec<Passage>) {
        let line_of = |at: usize| 1 + text[..at].matches('\n').count() as u32;
        let mut current: Option<(usize, usize, u32)> = None;
        for block in blocks.drain(..) {
            let len = text[block.start..block.end].chars().count();
            if len > MAX_CHARS {
                if let Some((s, e, l)) = current.take() {
                    out.push(Passage { heading: heading.to_string(), start: s, end: e, line: l });
                }
                let mut pieces = Vec::new();
                split_long(text, block.start, block.end, &mut pieces);
                for (s, e) in pieces {
                    let trimmed_end = s + text[s..e].trim_end().len();
                    out.push(Passage { heading: heading.to_string(), start: s, end: trimmed_end, line: line_of(s) });
                }
                continue;
            }
            match current {
                Some((s, e, l)) if text[s..e].chars().count() + len < TARGET_CHARS => current = Some((s, block.end, l)),
                Some((s, e, l)) => {
                    out.push(Passage { heading: heading.to_string(), start: s, end: e, line: l });
                    current = Some((block.start, block.end, block.line));
                }
                None => current = Some((block.start, block.end, block.line)),
            }
        }
        if let Some((s, e, l)) = current {
            out.push(Passage { heading: heading.to_string(), start: s, end: e, line: l });
        }
    }

    let mut offset = 0usize;
    let mut open: Option<Block> = None;
    let mut in_fence = false;
    for (index, raw) in text.split_inclusive('\n').enumerate() {
        let line_no = index as u32 + 1;
        let line_start = offset;
        offset += raw.len();
        let line = raw.trim_end_matches(['\n', '\r']);
        let content_end = line_start + line.len();
        if in_fence {
            if let Some(b) = open.as_mut() {
                b.end = content_end;
            }
            if is_fence(line) {
                in_fence = false;
                blocks.extend(open.take());
            }
            continue;
        }
        if let Some(h) = heading_of(line) {
            blocks.extend(open.take());
            flush(text, &heading, &mut blocks, &mut out);
            heading = h.to_string();
            continue;
        }
        if line.trim().is_empty() {
            blocks.extend(open.take());
            continue;
        }
        if is_fence(line) {
            blocks.extend(open.take());
            in_fence = true;
            open = Some(Block { start: line_start + (line.len() - line.trim_start().len()), end: content_end, line: line_no });
            continue;
        }
        match open.as_mut() {
            Some(b) => b.end = content_end,
            None => {
                let lead = line.len() - line.trim_start().len();
                open = Some(Block { start: line_start + lead, end: content_end, line: line_no });
            }
        }
    }
    blocks.extend(open.take());
    flush(text, &heading, &mut blocks, &mut out);
    out
}

/// The file's title: its first level-one heading when it opens with one (before any other
/// text), else the file name without its extension. PRD: *"Use the file title or filename."*
pub fn title(text: &str, file_stem: &str) -> String {
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("# ") {
            let rest = rest.trim().trim_end_matches('#').trim();
            if !rest.is_empty() {
                return rest.to_string();
            }
        }
        break;
    }
    file_stem.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_paragraphs_and_lines() {
        let text = "# Title\n\nIntro line one.\nIntro line two.\n\n## Refunds\n\nCancel within 30 days.\n\nSecond paragraph.\n";
        let p = passages(text);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].heading, "Title");
        assert_eq!(p[0].text(text), "Intro line one.\nIntro line two.");
        assert_eq!(p[0].line, 3);
        assert_eq!(p[1].heading, "Refunds");
        assert_eq!(p[1].text(text), "Cancel within 30 days.\n\nSecond paragraph.");
        assert_eq!(p[1].line, 8);
        assert_eq!(title(text, "file"), "Title");
        assert_eq!(title("No heading here\n# Later", "my-notes"), "my-notes");
    }

    #[test]
    fn paragraphs_join_up_to_the_target_and_then_start_a_new_passage() {
        let para = "word ".repeat(100); // 500 characters
        let text = format!("{para}\n\n{para}\n\n{para}\n");
        let p = passages(&text);
        assert_eq!(p.len(), 3, "500 + 500 passes the 700-character target, so each paragraph stands alone");
        let small = "short one.\n\nshort two.\n\nshort three.\n";
        assert_eq!(passages(small).len(), 1);
    }

    #[test]
    fn a_long_paragraph_is_cut_and_every_late_sentence_is_kept() {
        let sentence = "The quick brown fox jumps over the lazy dog again. ";
        let long = sentence.repeat(120) + "The very last answer is forty-two.";
        let p = passages(&long);
        assert!(p.len() > 4);
        for x in &p {
            assert!(x.text(&long).chars().count() <= MAX_CHARS);
        }
        assert!(p.last().unwrap().text(&long).ends_with("The very last answer is forty-two."));
        // Coverage: every non-space character is inside some passage.
        let covered: usize = p.iter().map(|x| x.text(&long).chars().filter(|c| !c.is_whitespace()).count()).sum();
        assert_eq!(covered, long.chars().filter(|c| !c.is_whitespace()).count());
    }

    #[test]
    fn a_fence_with_blank_lines_is_one_block_and_heading_marks_inside_it_are_code() {
        let text = "Before.\n\n```\nfn a() {}\n\n# not a heading\n```\n\nAfter.\n";
        let p = passages(text);
        assert_eq!(p.len(), 1);
        assert!(p[0].text(text).contains("# not a heading"));
        assert_eq!(p[0].heading, "");
    }

    #[test]
    fn multibyte_text_is_cut_on_character_boundaries() {
        let long = "äöü ".repeat(800);
        for x in passages(&long) {
            assert!(long.is_char_boundary(x.start) && long.is_char_boundary(x.end));
        }
    }
}
