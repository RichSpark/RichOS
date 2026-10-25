//! **AMERICAN SPELLING, FIXED MECHANICALLY IN THE TEXT RICHOS GIVES HIM** — no model involved.
//!
//! The CEO, 2026-09-27 (richos-hq `wiki/ceo-decisions.md` §93): *"The goal is to have a guard
//! that is as fast as possible and eliminates the need for slow inference as much as possible
//! i.e. automates as much as possible on this matter."* And from his input file: *"I just want
//! to eliminate British spelling from the front-end output."* 99% is his bar, not 100%.
//!
//! # The data
//!
//! One committed table, `engine/scripts/lib/dialect/dialect-en-US.generated.dict`, compiled
//! in with `include_str!` the way `richos-voice` compiles in `model-pins.json`. It is
//! generated from VarCon's verified clusters (a MODIFIED version of VarCon, and its header says
//! so; the notices ship with the app, see `docs/legal/THIRD-PARTY-NOTICES.md`). Every key and
//! every value is lowercase ASCII letters and no key is hex-only, so a commit SHA can never
//! match one (the suite checks both). A form that is also the preferred American spelling in
//! some sense is not in the table at all, so "ambiguous" is decided by the data, not here.
//!
//! # The context rules
//!
//! A word is changed only when all of these hold. Each is decided from the characters already
//! seen plus a few characters of lookahead, never by judgment:
//!
//! - it is outside fenced code (a line opening with three or more backticks or tildes, closed
//!   by a run at least as long), outside a code span (a backtick run, closed by a run of the
//!   same length), outside a `>` line, and outside a double-quoted run (straight or curly). A
//!   blank line resets the code-span and quotation state, so an unbalanced mark costs at most
//!   one paragraph of missed fixes, which is the safe direction. Single quotes are not tracked,
//!   because apostrophes make them undecidable.
//! - its token (the run of letters, digits and `_ $ . / : \ -` around it, the same token
//!   `dialect-guard.sh` uses) does not look like a path, URL, identifier or dotted name: no
//!   `/`, `\`, `_`, `$`, `::`, `--`, and no `.` followed by a letter. A bare single-hyphen
//!   compound in prose is still fixed, which the plan check accepts inside the 99%.
//! - it is a whole word: no digit, underscore or other letter (accented ones included)
//!   touching it.
//! - it is all lowercase, all capitals, or capitalized. A capitalized word is changed only at
//!   the start of a sentence, because a capital mid-sentence is a name (a party, a ministry, a
//!   venue). At the start of a sentence a capitalized word followed by a possessive `'s` is
//!   also left, because that is a name too (a television title heading a sentence). The one
//!   price, measured on the keyed sample: a capitalized common word mid-sentence, as in a
//!   title-case heading's second word, stays as written.
//!
//! # Streaming, and his first word
//!
//! [`Speller::push`] takes the deltas exactly as the model streams them and returns the text
//! that is decided. It holds back ONLY a word that could still change: a trailing letter run
//! that is the start of some key, or a key whose token has not ended (the next characters could
//! still make it a path). Every other character goes out at once, so a delta that ends in
//! whitespace or punctuation holds nothing, and a delta that ends inside a word holds that word
//! for one delta at most. Backtick and tilde runs are held until the run ends, because one
//! backtick and three backticks mean different things. [`Speller::finish`] releases everything;
//! the drain calls it on every exit from the turn.
//!
//! **Streaming never changes the answer.** Every decision is made only once the characters it
//! depends on have arrived, so the concatenated output of any split of a text equals [`fix`] of
//! the whole text (the suite proves it at every split point of the keyed sample). A
//! [`Speller::finish`] in the middle of a text (the drain calls it before a tool call is shown)
//! is an end of text, and that is the only way the two can differ.
//!
//! # Where it is applied
//!
//! - **Replies**, at the one drain where streamed text becomes a [`crate::native::TurnItem`]
//!   (`NativeClient::prompt`). Everything downstream of the ledger inherits the fixed text: the
//!   Mac's screen, speech, crash recovery, the phones and both kinds of push preview.
//! - **Documents the app's sessions write** (Write, Edit, MultiEdit and NotebookEdit into prose
//!   files), in the desktop executable's `PreToolUse` wrapper: [`fix_tool_input`].
//!
//! **His own words never pass through here.** His typed and spoken input is recorded by
//! `record_prompt_received*` and sent to the model as written; nothing on those paths calls this
//! module (the drain's tests prove the prompt reaches the child byte for byte).
//!
//! **The model's own transcript keeps what the model wrote** while the ledger holds the fixed
//! form. Nothing compares the two for display, so this is harmless; it is stated here so nobody
//! is surprised by it.

use std::collections::HashMap;
use std::sync::LazyLock;

/// The generated table, compiled in. Four levels up from this file is `richos/`.
const TABLE_SOURCE: &str = include_str!("../../../../engine/scripts/lib/dialect/dialect-en-US.generated.dict");

struct Table {
    pairs: HashMap<&'static str, &'static str>,
    /// Every key, sorted, for the prefix test.
    sorted: Vec<&'static str>,
}

static TABLE: LazyLock<Table> = LazyLock::new(|| {
    let mut pairs = HashMap::new();
    for line in TABLE_SOURCE.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split('\t');
        if let (Some(from), Some(to)) = (fields.next(), fields.next()) {
            if !from.is_empty() && !to.is_empty() {
                pairs.insert(from, to);
            }
        }
    }
    let mut sorted: Vec<&'static str> = pairs.keys().copied().collect();
    sorted.sort_unstable();
    Table { pairs, sorted }
});

impl Table {
    fn get(&self, lower: &str) -> Option<&'static str> {
        self.pairs.get(lower).copied()
    }
    /// Is `lower` a key, or the beginning of one?
    fn is_prefix(&self, lower: &str) -> bool {
        let at = self.sorted.partition_point(|k| *k < lower);
        self.sorted.get(at).is_some_and(|k| k.starts_with(lower))
    }
}

/// How many pairs the compiled-in table carries.
pub fn table_len() -> usize {
    TABLE.pairs.len()
}

/// Fix a whole text at once. Identical to pushing it through a [`Speller`] in any split.
pub fn fix(text: &str) -> String {
    let mut speller = Speller::new();
    let mut out = speller.push(text);
    out.push_str(&speller.finish());
    out
}

/// One change: the word as written, what it became, its 1-based line and its byte offset in
/// the text that was fixed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub from: String,
    pub to: String,
    pub line: usize,
    pub offset: usize,
}

#[cfg(test)]
thread_local! {
    /// How many whole-text scans this thread has run; a test counts them.
    static SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// [`fix`], plus the list of what changed. Used where the writer is told what was changed (a
/// document write), so a later edit of the same text quotes the text that is really there.
pub fn fix_with_changes(text: &str) -> (String, Vec<Change>) {
    #[cfg(test)]
    SCANS.with(|n| n.set(n.get() + 1));
    let mut speller = Speller::new();
    speller.changes = Some(Vec::new());
    let mut out = speller.push(text);
    out.push_str(&speller.finish());
    (out, speller.changes.take().unwrap_or_default())
}

/// The characters of a token: the same set `dialect-guard.sh` calls `TOKEN_CHARS`.
fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '.' | '/' | ':' | '\\' | '-')
}

/// A character that makes an adjacent letter run part of a longer word.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The shape of a letter run: only the first three are ever changed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shape {
    Lower,
    Capitalized,
    Upper,
    Mixed,
}

fn shape(run: &str) -> Shape {
    let mut chars = run.chars();
    let first_upper = chars.next().is_some_and(|c| c.is_ascii_uppercase());
    let rest: Vec<char> = chars.collect();
    let rest_lower = rest.iter().all(|c| c.is_ascii_lowercase());
    let rest_upper = rest.iter().all(|c| c.is_ascii_uppercase());
    match (first_upper, rest_lower, rest_upper) {
        (false, true, _) => Shape::Lower,
        (true, _, true) if !rest.is_empty() => Shape::Upper,
        (true, true, _) => Shape::Capitalized,
        _ => Shape::Mixed,
    }
}

fn recase(target: &str, shape: Shape) -> String {
    match shape {
        Shape::Upper => target.to_ascii_uppercase(),
        Shape::Capitalized => {
            let mut out = String::with_capacity(target.len());
            let mut chars = target.chars();
            if let Some(first) = chars.next() {
                out.push(first.to_ascii_uppercase());
            }
            out.extend(chars);
            out
        }
        _ => target.to_string(),
    }
}

/// Does this text contain a mark that makes its token a path, URL, identifier or dotted name?
/// Every pattern is at most two characters long, so checking the previous token character plus
/// the new ones finds a pattern that straddles a split.
fn has_skip_mark(text: &str) -> bool {
    let mut prev = '\0';
    for c in text.chars() {
        if matches!(c, '/' | '\\' | '_' | '$') {
            return true;
        }
        if (prev == ':' && c == ':') || (prev == '-' && c == '-') || (prev == '.' && c.is_ascii_alphabetic()) {
            return true;
        }
        prev = c;
    }
    false
}

/// What is known about the part of the current token that has already gone out.
#[derive(Clone, Copy, Debug, Default)]
struct OpenToken {
    /// The part already emitted made the token a path or identifier; nothing more is fixed.
    skip: bool,
    /// A letter or digit already emitted, so the next letter run is not the token's first.
    has_alnum: bool,
    /// The last emitted character.
    last: Option<char>,
}

/// The streaming fixer. One per turn.
#[derive(Debug, Default)]
pub struct Speller {
    pending: String,
    /// Input bytes consumed before `pending`, for [`Change::offset`].
    consumed: usize,
    /// The open fence: its character and its length.
    fence: Option<(char, usize)>,
    /// The open code span: its backtick run length.
    code_span: Option<usize>,
    in_quote: bool,
    blockquote: bool,
    /// Any non-whitespace character on the current line yet.
    line_has_content: bool,
    /// The next capitalized word starts a sentence. True at the start of the text.
    sentence_start: bool,
    /// The last character that went out, for the word boundary of the next token.
    prev: Option<char>,
    /// Set while a token has gone out in part.
    open: Option<OpenToken>,
    changes: Option<Vec<Change>>,
    /// The 1-based line of the next character that goes out.
    line: usize,
}

/// What one step of the scan decided.
enum Step {
    /// Consumed up to this byte offset of the pending text.
    To(usize),
    /// The next decision needs characters that have not arrived yet.
    Wait,
}

impl Speller {
    pub fn new() -> Self {
        Speller { sentence_start: true, line: 1, ..Default::default() }
    }

    /// Is any text being held back?
    pub fn is_holding(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Take one streamed delta and return the text that is now decided (possibly empty).
    pub fn push(&mut self, delta: &str) -> String {
        self.pending.push_str(delta);
        self.run(false)
    }

    /// The text has ended (the turn finished, stopped or failed, or a tool call is about to be
    /// shown): release everything held, deciding it as the end of the text.
    pub fn finish(&mut self) -> String {
        let out = self.run(true);
        // What follows is a new text (the next block after a tool call): it starts where a
        // text starts, with no fence, span or quotation carried into it.
        *self = Speller {
            consumed: self.consumed,
            line: self.line,
            changes: self.changes.take(),
            ..Speller::new()
        };
        out
    }

    fn run(&mut self, complete: bool) -> String {
        let text = std::mem::take(&mut self.pending);
        let mut out = String::with_capacity(text.len());
        let mut at = 0;
        while at < text.len() {
            match self.step(&text, at, complete, &mut out) {
                Step::To(next) => at = next,
                Step::Wait => break,
            }
        }
        self.consumed += at;
        self.pending = text[at..].to_string();
        out
    }

    fn emit(&mut self, out: &mut String, piece: &str) {
        if piece.is_empty() {
            return;
        }
        out.push_str(piece);
        self.line += piece.matches('\n').count();
        self.prev = piece.chars().next_back();
    }

    fn close_token(&mut self, open: OpenToken) {
        if open.has_alnum {
            self.sentence_start = matches!(open.last, Some('.' | ':'));
        }
        self.line_has_content = true;
    }

    fn step(&mut self, text: &str, at: usize, complete: bool, out: &mut String) -> Step {
        let c = text[at..].chars().next().expect("at is below the length");
        let next = at + c.len_utf8();

        // A token that went out in part, and this character does not continue it: it has ended.
        if self.open.is_some() && !is_token_char(c) {
            let open = self.open.take().expect("checked");
            self.close_token(open);
        }

        if let Some((fence_char, fence_len)) = self.fence {
            if c == '\n' {
                self.line_has_content = false;
                self.emit(out, "\n");
                return Step::To(next);
            }
            if !self.line_has_content && (c == ' ' || c == '\t') {
                self.emit(out, &text[at..next]);
                return Step::To(next);
            }
            if !self.line_has_content && c == fence_char {
                let Some(end) = run_end(text, at, c, complete) else { return Step::Wait };
                if text[at..end].chars().count() >= fence_len {
                    self.fence = None;
                    self.sentence_start = true;
                }
                self.line_has_content = true;
                self.emit(out, &text[at..end]);
                return Step::To(end);
            }
            self.line_has_content = true;
            self.emit(out, &text[at..next]);
            return Step::To(next);
        }

        match c {
            '\n' => {
                if !self.line_has_content {
                    // A blank line ends a paragraph: an unbalanced mark stops here.
                    self.in_quote = false;
                    self.code_span = None;
                }
                self.line_has_content = false;
                self.blockquote = false;
                self.sentence_start = true;
                self.emit(out, "\n");
                Step::To(next)
            }
            ' ' | '\t' | '\r' => {
                self.emit(out, &text[at..next]);
                Step::To(next)
            }
            '`' | '~' => {
                let Some(end) = run_end(text, at, c, complete) else { return Step::Wait };
                let run = text[at..end].chars().count();
                if !self.line_has_content && run >= 3 && self.code_span.is_none() {
                    self.fence = Some((c, run));
                } else if c == '`' {
                    self.code_span = match self.code_span {
                        None => Some(run),
                        Some(open) if open == run => None,
                        other => other,
                    };
                }
                self.line_has_content = true;
                self.emit(out, &text[at..end]);
                Step::To(end)
            }
            '>' if !self.line_has_content && self.code_span.is_none() => {
                self.blockquote = true;
                self.line_has_content = true;
                self.emit(out, ">");
                Step::To(next)
            }
            '"' | '\u{201C}' | '\u{201D}' => {
                if self.code_span.is_none() {
                    self.in_quote = match c {
                        '"' => !self.in_quote,
                        '\u{201C}' => true,
                        _ => false,
                    };
                }
                self.line_has_content = true;
                self.emit(out, &text[at..next]);
                Step::To(next)
            }
            '!' | '?' => {
                self.sentence_start = true;
                self.line_has_content = true;
                self.emit(out, &text[at..next]);
                Step::To(next)
            }
            c if is_token_char(c) => self.token(text, at, complete, out),
            _ => {
                self.line_has_content = true;
                self.emit(out, &text[at..next]);
                Step::To(next)
            }
        }
    }

    /// The token starting at `at` (or continuing one that went out in part).
    fn token(&mut self, text: &str, at: usize, complete: bool, out: &mut String) -> Step {
        let end = text[at..].find(|c: char| !is_token_char(c)).map_or(text.len(), |i| at + i);
        let ended = end < text.len() || complete;
        let seg = &text[at..end];
        let open = self.open.unwrap_or_default();
        let probe = match open.last {
            Some(last) => format!("{last}{seg}"),
            None => seg.to_string(),
        };
        let skip = open.skip || has_skip_mark(&probe);
        let context_open = self.code_span.is_none() && !self.in_quote && !self.blockquote;

        if !context_open || skip {
            // Nothing in this token can change, now or later: it goes out as it is.
            self.line_has_content = true;
            self.emit(out, seg);
            let merged = merge(open, seg, skip);
            if ended {
                self.open = None;
                self.close_token(merged);
            } else {
                self.open = Some(merged);
            }
            return Step::To(end);
        }

        let left = open.last.or(self.prev);
        let right = if ended { Boundary::Known(text[end..].chars().next()) } else { Boundary::Open };
        let words = self.words(seg, open, left, right);

        if !ended {
            // Emit up to the first word that could still change, and hold the rest.
            let cut = words.iter().find(|w| w.decision != Decision::Leave).map_or(seg.len(), |w| w.start);
            let emitted = &seg[..cut];
            if !emitted.is_empty() {
                self.line_has_content = true;
                self.emit(out, emitted);
                self.open = Some(merge(open, emitted, false));
            }
            return if cut == seg.len() { Step::To(end) } else { Step::To(at + cut).or_wait(cut) };
        }

        // The token has ended: decide every word in it.
        let mut fixed = String::with_capacity(seg.len());
        let mut last = 0;
        let mut made = Vec::new();
        for word in &words {
            let Decision::Change(target) = word.decision else { continue };
            // The possessive rule needs up to three characters after the token.
            if word.capitalized_at_sentence_start && word.end == seg.len() {
                match possessive_follows(&text[end..], complete) {
                    None => return Step::Wait,
                    Some(true) => continue,
                    Some(false) => {}
                }
            }
            let written = &seg[word.start..word.end];
            let replacement = recase(target, shape(written));
            fixed.push_str(&seg[last..word.start]);
            fixed.push_str(&replacement);
            last = word.end;
            made.push(Change {
                from: written.to_string(),
                to: replacement,
                line: self.line,
                offset: self.consumed + at + word.start,
            });
        }
        fixed.push_str(&seg[last..]);
        if let Some(changes) = self.changes.as_mut() {
            changes.extend(made);
        }
        self.line_has_content = true;
        self.emit(out, &fixed);
        self.open = None;
        self.close_token(merge(open, seg, false));
        Step::To(end)
    }

    /// The letter runs of `seg`, each with what would be done to it.
    fn words(&self, seg: &str, open: OpenToken, left: Option<char>, right: Boundary) -> Vec<Word> {
        let mut words = Vec::new();
        let bytes = seg.as_bytes();
        let mut i = 0;
        let mut seen_alnum = open.has_alnum;
        while i < bytes.len() {
            if !bytes[i].is_ascii_alphabetic() {
                if bytes[i].is_ascii_digit() {
                    seen_alnum = true;
                }
                i += 1;
                continue;
            }
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            let end = i;
            let run = &seg[start..end];
            let before = if start == 0 { left } else { seg[..start].chars().next_back() };
            let first_of_token = !seen_alnum;
            seen_alnum = true;
            // Letters that continue a run that already went out: that run was not a key or the
            // start of one, so no longer run beginning with it can be one either.
            let blocked_left = before.is_some_and(is_word_char);
            let at_sentence_start = first_of_token && self.sentence_start;
            let run_shape = shape(run);
            let lower = run.to_ascii_lowercase();
            let decision = if blocked_left || run_shape == Shape::Mixed {
                Decision::Leave
            } else if let (true, Boundary::Open) = (end == seg.len(), right) {
                // The run may still grow. It is held only if it could become a word that
                // changes: a single capital may still grow into all capitals.
                let capital_allowed = run_shape != Shape::Capitalized || at_sentence_start || run.len() == 1;
                if capital_allowed && TABLE.is_prefix(&lower) { Decision::Undecided } else { Decision::Leave }
            } else {
                let after = if end == seg.len() {
                    match right {
                        Boundary::Known(c) => c,
                        Boundary::Open => None,
                    }
                } else {
                    seg[end..].chars().next()
                };
                let blocked_right = after.is_some_and(is_word_char);
                let capital_allowed = run_shape != Shape::Capitalized || at_sentence_start;
                match TABLE.get(&lower) {
                    Some(target) if !blocked_right && capital_allowed => Decision::Change(target),
                    _ => Decision::Leave,
                }
            };
            words.push(Word {
                start,
                end,
                decision,
                capitalized_at_sentence_start: run_shape == Shape::Capitalized && at_sentence_start,
            });
        }
        // While the token has not ended, a complete key inside it is still undecided: the rest
        // of the token could make it a path.
        if let Boundary::Open = right {
            for word in &mut words {
                if let Decision::Change(_) = word.decision {
                    word.decision = Decision::Undecided;
                }
            }
        }
        words
    }
}

impl Step {
    /// A cut at zero consumes nothing: that is a wait, not a step.
    fn or_wait(self, cut: usize) -> Step {
        if cut == 0 { Step::Wait } else { self }
    }
}

#[derive(Clone, Copy)]
enum Boundary {
    /// The token has ended; this is the character after it (`None` at the end of the text).
    Known(Option<char>),
    /// The token has not ended yet.
    Open,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Decision {
    Leave,
    Undecided,
    Change(&'static str),
}

struct Word {
    start: usize,
    end: usize,
    decision: Decision,
    capitalized_at_sentence_start: bool,
}

fn merge(open: OpenToken, emitted: &str, skip: bool) -> OpenToken {
    OpenToken {
        skip: open.skip || skip,
        has_alnum: open.has_alnum || emitted.chars().any(|c| c.is_ascii_alphanumeric()),
        last: emitted.chars().next_back().or(open.last),
    }
}

/// The end of the run of `c` starting at `at`, or `None` if it reaches the end of the text and
/// more text may follow.
fn run_end(text: &str, at: usize, c: char, complete: bool) -> Option<usize> {
    let end = text[at..].find(|x: char| x != c).map_or(text.len(), |i| at + i);
    if end == text.len() && !complete { None } else { Some(end) }
}

/// Is the token followed by a possessive `'s` (straight or curly apostrophe) that ends there?
/// `None` means the characters that decide it have not arrived.
fn possessive_follows(rest: &str, complete: bool) -> Option<bool> {
    let mut chars = rest.chars();
    let undecided = if complete { Some(false) } else { None };
    match chars.next() {
        None => return undecided,
        Some('\'' | '\u{2019}') => {}
        Some(_) => return Some(false),
    }
    match chars.next() {
        None => return undecided,
        Some('s' | 'S') => {}
        Some(_) => return Some(false),
    }
    match chars.next() {
        None => {
            if complete {
                Some(true)
            } else {
                None
            }
        }
        Some(c) => Some(!is_word_char(c)),
    }
}

// =============================================================================================
// DOCUMENTS THE APP'S SESSIONS WRITE
// =============================================================================================

/// The file classes a document write is fixed in: prose only. Code, markup, styles and
/// configuration are never changed here, because a wrong fix there changes behavior with nobody
/// told (plan check catch C3), and a user's own code is his to spell.
pub const PROSE_EXTENSIONS: &[&str] = &["md", "markdown", "mdx", "txt", "text", "rst"];

fn is_prose_path(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| PROSE_EXTENSIONS.iter().any(|p| p.eq_ignore_ascii_case(e)))
}

/// A path segment that marks deliberately British or somebody else's material: test inputs
/// and vendored trees.
fn is_exempt_path(path: &str) -> bool {
    std::path::Path::new(path).components().any(|c| {
        matches!(c.as_os_str().to_str(), Some("fixtures" | "third_party" | "vendor" | "node_modules"))
    })
}

/// The fixed tool input for a `PreToolUse` callback and the changes made, or `None` when there
/// is nothing to change. Only the text a write introduces changes (`content`, `new_string`,
/// `new_source`); `old_string` never does, so an Edit still finds its text.
///
/// An Edit or MultiEdit fragment carries no file context of its own, so the file is read and
/// the fragment is fixed as it will sit in the file: a word inside a fence, a code span, a
/// quotation or a `>` line there is left, exactly as it would be in the whole file. A fragment
/// whose `old_string` is not in the file is left alone, and the Edit then fails on its own
/// (plan check catch C4). A notebook's markdown cells are prose; its code cells are not.
pub fn fix_tool_input(tool_name: &str, input: &serde_json::Value) -> Option<(serde_json::Value, Vec<Change>)> {
    let path = input.get("file_path").or_else(|| input.get("notebook_path")).and_then(|v| v.as_str())?;
    if is_exempt_path(path) {
        return None;
    }
    let mut fixed = input.clone();
    let mut all = Vec::new();
    match tool_name {
        "Write" => {
            if !is_prose_path(path) {
                return None;
            }
            let (text, changes) = fix_with_changes(input.get("content")?.as_str()?);
            fixed["content"] = serde_json::Value::String(text);
            all.extend(changes);
        }
        "Edit" => {
            if !is_prose_path(path) {
                return None;
            }
            let old = input.get("old_string")?.as_str()?;
            let new = input.get("new_string")?.as_str()?;
            let every = input.get("replace_all").and_then(|v| v.as_bool()) == Some(true);
            let file = std::fs::read_to_string(path).ok()?;
            let (text, changes) = fix_fragment_at(&file, old, new, every)?;
            fixed["new_string"] = serde_json::Value::String(text);
            all.extend(changes);
        }
        "MultiEdit" => {
            if !is_prose_path(path) {
                return None;
            }
            let mut file = std::fs::read_to_string(path).ok()?;
            let edits = input.get("edits")?.as_array()?;
            let mut out = Vec::with_capacity(edits.len());
            for edit in edits {
                let mut edit = edit.clone();
                let old = edit.get("old_string").and_then(|v| v.as_str()).map(str::to_string);
                let new = edit.get("new_string").and_then(|v| v.as_str()).map(str::to_string);
                let every = edit.get("replace_all").and_then(|v| v.as_bool()) == Some(true);
                if let (Some(old), Some(new)) = (old, new) {
                    let text = match fix_fragment_at(&file, &old, &new, every) {
                        Some((text, changes)) => {
                            edit["new_string"] = serde_json::Value::String(text.clone());
                            all.extend(changes);
                            text
                        }
                        None => new,
                    };
                    // Later edits see the file as the earlier ones leave it.
                    file = if every { file.replace(&old, &text) } else { file.replacen(&old, &text, 1) };
                }
                out.push(edit);
            }
            fixed["edits"] = serde_json::Value::Array(out);
        }
        "NotebookEdit" => {
            if input.get("cell_type").and_then(|v| v.as_str()) != Some("markdown") {
                return None;
            }
            let (text, changes) = fix_with_changes(input.get("new_source")?.as_str()?);
            fixed["new_source"] = serde_json::Value::String(text);
            all.extend(changes);
        }
        _ => return None,
    }
    if all.is_empty() { None } else { Some((fixed, all)) }
}

/// The ONE `PreToolUse` envelope for a fixed document write: the canonical hook's own output
/// (`app-engine-hook.py` prints at most one JSON object, the worker context) with
/// `updatedInput` and a note of every change merged into it. Never a second envelope, never
/// `permissionDecision` (setting it would approve the write and skip the permission flow;
/// plan check C18). `None` means "print the canonical output unchanged": it is not one JSON
/// object, or it already decides or transforms the call, and a spelling fix never overrides
/// either.
pub fn merge_into_envelope(canonical: &[u8], fixed: serde_json::Value, changes: &[Change]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(canonical).ok()?.trim();
    let mut envelope: serde_json::Value =
        if text.is_empty() { serde_json::json!({}) } else { serde_json::from_str(text).ok()? };
    let object = envelope.as_object_mut()?;
    if object.contains_key("decision") || object.get("continue") == Some(&serde_json::Value::Bool(false)) {
        return None;
    }
    let specific = object.entry("hookSpecificOutput").or_insert_with(|| serde_json::json!({})).as_object_mut()?;
    if specific.contains_key("updatedInput") || specific.contains_key("permissionDecision") {
        return None;
    }
    specific.insert("hookEventName".into(), "PreToolUse".into());
    specific.insert("updatedInput".into(), fixed);
    let note = change_note(changes);
    let context = match specific.get("additionalContext").and_then(|v| v.as_str()) {
        Some(existing) if !existing.is_empty() => format!("{existing}\n\n{note}"),
        _ => note,
    };
    specific.insert("additionalContext".into(), context.into());
    let mut bytes = serde_json::to_vec(&envelope).ok()?;
    bytes.push(b'\n');
    Some(bytes)
}

/// What the writer is told, so a later Edit quotes the words that are really in the file (plan
/// check C7: a silent fix would remove the signal a refusal used to give).
fn change_note(changes: &[Change]) -> String {
    const SHOWN: usize = 20;
    let listed: Vec<String> =
        changes.iter().take(SHOWN).map(|c| format!("{} -> {} (line {})", c.from, c.to, c.line)).collect();
    let more = changes.len().saturating_sub(SHOWN);
    let more = if more > 0 { format!(", and {more} more") } else { String::new() };
    format!(
        "American spelling: this write was changed before it ran, {} word(s): {}{more}. \
         The file holds the American forms; quote those in any later edit.",
        changes.len(),
        listed.join(", ")
    )
}

/// Fix `new` as it will sit in `file` in place of `old`: the whole file as it will be is fixed,
/// and only the changes that fall wholly inside `new` are kept. `None` if `old` is not in the
/// file or nothing inside `new` changes. With `every`, each place `old` occurs must give the
/// same answer, or the fragment is left alone rather than guessed.
fn fix_fragment_at(file: &str, old: &str, new: &str, every: bool) -> Option<(String, Vec<Change>)> {
    if old.is_empty() {
        return None;
    }
    let places: Vec<usize> = if every {
        file.match_indices(old).map(|(i, _)| i).collect()
    } else {
        vec![file.find(old)?]
    };
    // **One scan of the document as it will be, not one per place.** Every place gets `new`
    // (with `every` that is exactly what the edit will leave in the file), the whole result is
    // checked once, and each place's own changes are read out of it by offset. The context that
    // protects fences and quotations is the same context; it is just computed once.
    let mut whole = String::with_capacity(file.len() + places.len() * new.len().saturating_sub(old.len()));
    let mut starts = Vec::with_capacity(places.len());
    let mut from = 0;
    for &offset in &places {
        whole.push_str(&file[from..offset]);
        starts.push(whole.len());
        whole.push_str(new);
        from = offset + old.len();
    }
    whole.push_str(&file[from..]);
    let (_, changes) = fix_with_changes(&whole);
    let mut answer: Option<(String, Vec<Change>)> = None;
    let mut next = 0; // changes are in offset order; places are too
    for &offset in &starts {
        while next < changes.len() && changes[next].offset < offset {
            next += 1;
        }
        let inside: Vec<Change> = changes[next..]
            .iter()
            .take_while(|c| c.offset < offset + new.len())
            .filter(|c| c.offset + c.from.len() <= offset + new.len())
            .cloned()
            .collect();
        let mut text = String::with_capacity(new.len());
        let mut last = 0;
        for change in &inside {
            let at = change.offset - offset;
            text.push_str(&new[last..at]);
            text.push_str(&change.to);
            last = at + change.from.len();
        }
        text.push_str(&new[last..]);
        match &answer {
            None => answer = Some((text, inside)),
            Some((first, _)) if *first == text => {}
            Some(_) => return None,
        }
    }
    answer.filter(|(text, changes)| text != new && !changes.is_empty())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The keyed British sample: `{british|american}` must change, `{word|=}` must stay. It lives
    /// in the declared British test-input location, which the write guard exempts.
    const KEYED: &str = include_str!("../../../../engine/scripts/lib/dialect/fixtures/british-keyed.txt");
    /// The traps the table carries, so only context keeps them.
    const TRAPS: &str =
        include_str!("../../../../engine/scripts/lib/dialect/fixtures/british-keyed.context-traps.txt");
    /// Stream A's own British inputs, in named sections.
    const STREAM: &str = include_str!("../../../../engine/scripts/lib/dialect/fixtures/stream-a-app.txt");

    /// (plain British text, expected text, must-fix pairs, must-leave words)
    fn keyed() -> (String, String, Vec<(String, String)>, Vec<String>) {
        let mut british = String::new();
        let mut expected = String::new();
        let mut fixes = Vec::new();
        let mut leaves = Vec::new();
        let mut rest = KEYED;
        while let Some(open) = rest.find('{') {
            british.push_str(&rest[..open]);
            expected.push_str(&rest[..open]);
            let close = rest[open..].find('}').expect("balanced key") + open;
            let (word, answer) = rest[open + 1..close].split_once('|').expect("keyed item");
            british.push_str(word);
            if answer == "=" {
                expected.push_str(word);
                leaves.push(word.to_string());
            } else {
                expected.push_str(answer);
                fixes.push((word.to_string(), answer.to_string()));
            }
            rest = &rest[close + 1..];
        }
        british.push_str(rest);
        expected.push_str(rest);
        (british, expected, fixes, leaves)
    }

    /// A named section of the stream-A fixture: the text between `=== name` and the next `===`.
    pub(crate) fn section(name: &str) -> String {
        let marker = format!("=== {name}\n");
        let start = STREAM.find(&marker).unwrap_or_else(|| panic!("fixture section {name}")) + marker.len();
        let end = STREAM[start..].find("\n=== ").map_or(STREAM.len(), |i| start + i + 1);
        STREAM[start..end].to_string()
    }

    fn words(text: &str) -> Vec<&str> {
        text.split(|c: char| !c.is_alphanumeric() && c != '\'').filter(|w| !w.is_empty()).collect()
    }

    fn streamed(text: &str, cut: usize) -> String {
        let mut speller = Speller::new();
        let mut out = speller.push(&text[..cut]);
        out.push_str(&speller.push(&text[cut..]));
        out.push_str(&speller.finish());
        out
    }

    #[test]
    fn the_table_is_the_generated_one_and_every_pair_is_plain_lowercase_ascii() {
        assert_eq!(table_len(), 3802, "the compiled-in table is not the committed generated table");
        for (from, to) in TABLE.pairs.iter() {
            assert!(from.bytes().all(|b| b.is_ascii_lowercase()), "key {from:?}");
            assert!(to.bytes().all(|b| b.is_ascii_lowercase()), "value {to:?}");
            // A key made of hex digits only could rewrite a commit SHA.
            assert!(!from.bytes().all(|b| b.is_ascii_hexdigit()), "hex-only key {from:?}");
        }
    }

    #[test]
    fn the_keyed_sample_leaves_every_trap_and_misses_only_the_declared_word() {
        let (british, expected, fixes, leaves) = keyed();
        assert_eq!((fixes.len(), leaves.len()), (106, 30), "the keyed sample changed shape");
        let got = fix(&british);
        let got_words = words(&got);
        let want_words = words(&expected);
        assert_eq!(got_words.len(), want_words.len(), "fixing added or removed words");
        let mut misses = Vec::new();
        let mut broken = Vec::new();
        for (g, w) in got_words.iter().zip(&want_words) {
            if g != w {
                if leaves.iter().any(|l| l == w) {
                    broken.push(format!("{w} -> {g}"));
                } else {
                    misses.push(format!("{g} (want {w})"));
                }
            }
        }
        assert!(broken.is_empty(), "traps broken: {broken:?}");
        // The declared misses: a capitalized word mid-sentence is read as a name. That is the
        // trade the research measured (one miss against three traps saved).
        let declared: Vec<String> = section("declared-misses").lines().map(str::to_string).collect();
        assert_eq!(misses, declared, "105 of 106 fixed is the measured floor");
    }

    #[test]
    fn every_context_trap_the_table_carries_survives_its_context() {
        let (british, _, _, _) = keyed();
        let got = fix(&british);
        for line in TRAPS.lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
            assert!(TABLE.get(line).is_some(), "{line} is listed as a trap the table carries");
        }
        for context in section("keyed-contexts").lines().filter(|l| !l.is_empty()) {
            assert!(got.contains(context), "context not left as written: {context}");
        }
    }

    #[test]
    fn streamed_at_every_split_point_equals_the_whole_text() {
        let (british, _, _, _) = keyed();
        let text = format!("{british}\n{}", section("streaming"));
        let whole = fix(&text);
        for cut in (0..=text.len()).filter(|i| text.is_char_boundary(*i)) {
            assert_eq!(streamed(&text, cut), whole, "split at byte {cut} changed the result");
        }
    }

    #[test]
    fn streamed_in_small_deltas_equals_the_whole_text() {
        let (british, _, _, _) = keyed();
        let text = format!("{british}\n{}", section("streaming"));
        let whole = fix(&text);
        for width in [1usize, 2, 3, 5, 7, 13] {
            let mut speller = Speller::new();
            let mut out = String::new();
            let mut piece = String::new();
            for c in text.chars() {
                piece.push(c);
                if piece.len() >= width {
                    out.push_str(&speller.push(&piece));
                    piece.clear();
                }
            }
            out.push_str(&speller.push(&piece));
            out.push_str(&speller.finish());
            assert_eq!(out, whole, "{width}-byte deltas changed the result");
        }
    }

    #[test]
    fn a_word_split_across_deltas_is_fixed_once_it_is_whole() {
        let parts: Vec<String> = section("split-word").lines().map(str::to_string).collect();
        let (first, second, want) = (&parts[0], &parts[1], &parts[2]);
        let mut speller = Speller::new();
        let mut out = speller.push(first);
        assert!(speller.is_holding(), "the start of a table word must be held");
        out.push_str(&speller.push(second));
        out.push_str(&speller.finish());
        assert_eq!(&out, want);
    }

    #[test]
    fn his_first_word_is_not_held_when_it_cannot_become_a_table_word() {
        // A delta that ends in whitespace or punctuation holds nothing.
        let mut speller = Speller::new();
        assert_eq!(speller.push("On it! "), "On it! ");
        assert!(!speller.is_holding());
        // A word that is not the start of any key goes out while its token is still open.
        let mut speller = Speller::new();
        assert_eq!(speller.push("Yes"), "Yes");
        assert!(!speller.is_holding());
        // A capital mid-sentence cannot change, so it is not held either.
        let mut speller = Speller::new();
        assert_eq!(speller.push("see the Col"), "see the Col");
        assert!(!speller.is_holding());
    }

    #[test]
    fn the_hold_is_only_the_trailing_possible_word_and_finish_releases_it() {
        let mut speller = Speller::new();
        assert_eq!(speller.push("I think the"), "I think ");
        assert!(speller.is_holding());
        assert_eq!(speller.finish(), "the");
        assert!(!speller.is_holding());
    }

    #[test]
    fn code_spans_fences_paths_urls_identifiers_names_and_quotations_are_left_as_written() {
        let text = section("contexts-left");
        let got = fix(&text);
        for (g, w) in got.lines().zip(text.lines()) {
            assert_eq!(g, w, "a protected context was changed");
        }
        assert_eq!(got, text);
    }

    #[test]
    fn prose_around_protected_contexts_is_still_fixed() {
        assert_eq!(fix(&section("contexts-mixed")), section("contexts-mixed-fixed"));
    }

    #[test]
    fn a_lone_backtick_and_a_fence_split_across_deltas_mean_what_they_mean_whole() {
        for name in ["contexts-mixed", "contexts-left", "case"] {
            let text = section(name);
            let whole = fix(&text);
            for cut in (0..=text.len()).filter(|i| text.is_char_boundary(*i)) {
                assert_eq!(streamed(&text, cut), whole, "{name}: split at {cut}");
            }
        }
    }

    #[test]
    fn case_is_kept_capitals_and_sentence_starts() {
        let got = fix(&section("case"));
        for (g, w) in got.lines().zip(section("case-fixed").lines()) {
            assert_eq!(g, w);
        }
    }

    #[test]
    fn nothing_but_whole_table_words_ever_changes() {
        let (british, _, _, _) = keyed();
        let got = fix(&british);
        // The runs between letter runs, in order: a changed word changes its length, never these.
        let between = |t: &str| -> Vec<String> {
            t.split(|c: char| c.is_ascii_alphabetic()).filter(|s| !s.is_empty()).map(str::to_string).collect()
        };
        assert_eq!(between(&british), between(&got), "a character outside a word changed");
        assert_eq!(words(&british).len(), words(&got).len(), "a word was added or removed");
    }

    #[test]
    fn text_with_nothing_to_change_is_returned_byte_for_byte() {
        let text = "Plain American text, a SHA 64ded37b, a path /tmp/x.md and `code`.\n\n```\nfn main() {}\n```\n";
        assert_eq!(fix(text), text);
        for cut in (0..=text.len()).filter(|i| text.is_char_boundary(*i)) {
            assert_eq!(streamed(text, cut), text);
        }
    }

    #[test]
    fn the_change_report_names_each_change_and_its_line() {
        let text = section("report");
        let (fixed, changes) = fix_with_changes(&text);
        assert_ne!(fixed, text);
        assert_eq!(changes.len(), 5, "{changes:?}");
        for change in &changes {
            assert_eq!(&text[change.offset..change.offset + change.from.len()], change.from);
            assert!(fixed.lines().nth(change.line - 1).unwrap().contains(&change.to), "{change:?}");
            assert!(text.lines().nth(change.line - 1).unwrap().contains(&change.from), "{change:?}");
        }
        assert_eq!(changes.iter().map(|c| c.line).collect::<Vec<_>>(), vec![1, 1, 3, 3, 3]);
    }

    // ---- documents ---------------------------------------------------------------------------

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("richos-american-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_prose_document_write_is_fixed_and_code_markup_and_config_are_not() {
        let dir = scratch("write");
        let body = section("report");
        let md = dir.join("summary.md");
        let input = serde_json::json!({"file_path": md.to_str().unwrap(), "content": body});
        let (fixed, changes) = fix_tool_input("Write", &input).expect("a prose write with table words is fixed");
        assert_eq!(fixed["content"].as_str().unwrap(), fix(&body));
        assert_eq!(fixed["file_path"], input["file_path"], "only the text changes");
        assert_eq!(changes.len(), 5);
        for ext in ["rs", "js", "html", "css", "json", "toml", "yaml", "swift", "kt", "svelte", "xml"] {
            let path = dir.join(format!("file.{ext}"));
            let input = serde_json::json!({"file_path": path.to_str().unwrap(), "content": body});
            assert!(fix_tool_input("Write", &input).is_none(), ".{ext} must never be changed");
        }
        let exempt = dir.join("fixtures").join("sample.md");
        let input = serde_json::json!({"file_path": exempt.to_str().unwrap(), "content": body});
        assert!(fix_tool_input("Write", &input).is_none(), "a fixtures path is British on purpose");
        let clean = serde_json::json!({"file_path": md.to_str().unwrap(), "content": "Plain words only."});
        assert!(fix_tool_input("Write", &clean).is_none(), "nothing to change is no output at all");
        assert!(fix_tool_input("Bash", &serde_json::json!({"command": "echo"})).is_none());
        drop(std::fs::remove_dir_all(&dir));
    }

    #[test]
    fn replacing_every_occurrence_scans_the_document_once_not_once_per_occurrence() {
        let dir = scratch("every");
        let md = dir.join("notes.md");
        let word = section("edit-word");
        let word = word.trim();
        let fixed_word = fix(word);
        let occurrences = 400;
        let file = "ANCHOR and some prose around it.\n".repeat(occurrences);
        std::fs::write(&md, &file).unwrap();
        let input = serde_json::json!({
            "file_path": md.to_str().unwrap(), "old_string": "ANCHOR",
            "new_string": format!("the {word}"), "replace_all": true});
        let before = SCANS.with(|n| n.get());
        let (out, changes) = fix_tool_input("Edit", &input).expect("every place is prose");
        let scans = SCANS.with(|n| n.get()) - before;
        assert_eq!(out["new_string"].as_str().unwrap(), format!("the {fixed_word}"));
        assert_eq!(changes.len(), 1, "the answer is the one fragment's changes, as before");
        assert_eq!(scans, 1, "{occurrences} occurrences must cost one document scan, not {scans}");
        drop(std::fs::remove_dir_all(&dir));
    }

    #[test]
    fn an_edit_is_fixed_in_its_place_in_the_file_and_left_inside_a_fence_or_quotation() {
        let dir = scratch("edit");
        let md = dir.join("notes.md");
        std::fs::write(&md, section("edit-file")).unwrap();
        let word = section("edit-word");
        let word = word.trim();
        let fixed_word = fix(word);
        assert_ne!(word, fixed_word);
        let path = md.to_str().unwrap();
        // In prose: fixed, and old_string untouched.
        let input = serde_json::json!({"file_path": path, "old_string": "PROSE-ANCHOR", "new_string": format!("the {word} here")});
        let (out, _) = fix_tool_input("Edit", &input).expect("prose edit");
        assert_eq!(out["new_string"].as_str().unwrap(), format!("the {fixed_word} here"));
        assert_eq!(out["old_string"], input["old_string"]);
        // Inside a fenced block and inside a quotation: left alone.
        for anchor in ["FENCE-ANCHOR", "QUOTE-ANCHOR"] {
            let input = serde_json::json!({"file_path": path, "old_string": anchor, "new_string": format!("the {word} here")});
            assert!(fix_tool_input("Edit", &input).is_none(), "{anchor}: must be left");
        }
        // Not found: left alone, and the Edit fails on its own.
        let input = serde_json::json!({"file_path": path, "old_string": "NOT-IN-THE-FILE", "new_string": format!("the {word}")});
        assert!(fix_tool_input("Edit", &input).is_none());
        // A fragment that opens a fence of its own: the word after the fence mark is code.
        let input = serde_json::json!({"file_path": path, "old_string": "PROSE-ANCHOR", "new_string": format!("\n```\n{word}\n```\n")});
        assert!(fix_tool_input("Edit", &input).is_none());
        // MultiEdit: each edit in its own place.
        let input = serde_json::json!({"file_path": path, "edits": [
            {"old_string": "PROSE-ANCHOR", "new_string": format!("a {word}")},
            {"old_string": "FENCE-ANCHOR", "new_string": format!("a {word}")}]});
        let (out, changes) = fix_tool_input("MultiEdit", &input).expect("one of the two is prose");
        assert_eq!(out["edits"][0]["new_string"].as_str().unwrap(), format!("a {fixed_word}"));
        assert_eq!(out["edits"][1]["new_string"].as_str().unwrap(), format!("a {word}"));
        assert_eq!(changes.len(), 1);
        // A notebook's markdown cell is prose; its code cell is not.
        let nb = dir.join("book.ipynb");
        let nb = nb.to_str().unwrap();
        let md_cell = serde_json::json!({"notebook_path": nb, "cell_type": "markdown", "new_source": format!("A {word}.")});
        assert!(fix_tool_input("NotebookEdit", &md_cell).is_some());
        let code_cell = serde_json::json!({"notebook_path": nb, "cell_type": "code", "new_source": format!("# {word}")});
        assert!(fix_tool_input("NotebookEdit", &code_cell).is_none());
        drop(std::fs::remove_dir_all(&dir));
    }
}
