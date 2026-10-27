//! **BUST A BUG** — the core half. The CEO's §115 (2026-10-09), and the acceptance criterion:
//!
//! > *"When the user has Claude set up in the app (which is the expected default), we should just
//! > let the user say what's wrong and where and let their Rich check and articulate everything
//! > properly and then submit a GitHub issue on their behalf."* On whose account: *"when they have
//! > a GitHub account, the issue submission should happen from their GitHub account. And if they
//! > don't, then we can use a RichOS reporting account."*
//!
//! And on what is private (§115, later the same day): *"If it's so hard to automate this
//! particular part, then just let's the user's Rich check and clean-up what needs to be
//! cleaned-up in that particular user's case."* The order, as he set it: **the scanner runs
//! first, Rich runs last**, and nothing after Rich changes the words.
//!
//! The design is round 21 (`richos-hq/design/mockups/rounds/round-21/`, approved "go!"). This
//! file is everything about it that is not a window or a socket, so all of it is tested here:
//!
//!   - **The scanner, first** ([`Scrubber`]). Names RichOS holds (conversations, companies,
//!     people, folders), file paths, email addresses and every word Rich named private are replaced
//!     by plain stand-ins such as `[a conversation]`. Each stand-in keeps what it replaced in
//!     [`Segment::was`] for the tooltip only the user sees; `was` is never part of the issue. It is
//!     an extra help, and it cannot be relied on: no rule can know an ordinary client's name.
//!   - **Rich's write-up** ([`writer_prompt`], [`writer_args`], [`parse_written`], [`draft_from`]).
//!     One `claude --print` turn with no tools, under the account the conversation runs on, asked
//!     to write the report with this user's private details already left out.
//!   - **Rich, last** ([`finish_prompt`], [`finished`]). What the scanner made of the report is
//!     given to Rich once more, beside the report as it was before the scanner. He has the final
//!     say: he puts back what the scanner hid that is not private in this user's case, and cleans
//!     up whatever private detail of it is still there. **The card shows his words, and the issue
//!     is those words**, with only the escaping GitHub needs. **No report is offered for sending
//!     without him** ([`checked`]): when he cannot be had (an error, a timeout, an answer that is
//!     not a report, a section of it that is not all there), the user's words are kept on
//!     this Mac and Rich is asked again by himself until he answers ([`Outbox::keep_unless_checked`],
//!     [`Outbox::check_due`]).
//!   - **The issue, word for word** ([`Sheet`], [`issue_body`], [`issue_request`]). What goes to
//!     GitHub is rendered from the sheet the user approved and nothing else: no label, no footer,
//!     no attachment (round 21 "Left out, on purpose": no screenshot).
//!   - **What leaves this Mac, decided once** ([`at_send`], [`Public`]). The words sent are the
//!     card's, and they are words Rich gave last, to the character ([`Check`]). Words the user
//!     changed (by hand, or by telling Rich) go through the scanner and then Rich first, and his
//!     version is what is shown and sent. [`Outbox::send`] takes only what `at_send` made.
//!   - **Send, and keep what could not go** ([`Outbox`]). The report is written to this Mac BEFORE
//!     the request is made, so a failure, a quit or a crash leaves it here exactly as approved. A
//!     waiting report is tried again by itself ([`Outbox::send_due`]) and at once on *Try now*.
//!
//! # Which account
//!
//! [`Credentials`] answers "who is this filed as" at send time. The app has no GitHub sign-in yet
//! (§20 is not built), so the shell's implementation answers [`Account::Reporting`]: RichOS's own
//! reporting relay, whose GitHub App files it, with no token on the Mac. A signed-in user's own
//! account slots in as [`Account::User`] in that one place; the request, the outbox and the card
//! already carry the account rather than assume it.
//!
//! # What this file does not do
//!
//! It opens no connection and starts no process. The shell (`src-tauri/src/bug_report.rs`) runs
//! `claude` and makes the HTTPS requests through the [`Transport`] seam, so nothing here can reach
//! GitHub and the tests below the shell cannot either.

use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use unicode_normalization::char::canonical_combining_class;
use unicode_normalization::UnicodeNormalization;

/// RichOS's repository: `github.com/RichCore/RichOS` from the GitHub move on (CEO, 2026-10-27),
/// `github.com/WebDevBooster/richos` before it ([`crate::github_home`]). The relay files reports and
/// ideas wherever it is switched to, and the card shows the link the relay answered with, so a
/// report sent before the move keeps its own link ([`page_to_open`]).
pub const REPOSITORY: &str = crate::github_home::HOME;

// ---------------------------------------------------------------------------------------
// TWO KINDS OF POST: A BUG REPORT, AND A FEATURE IDEA
// ---------------------------------------------------------------------------------------

/// **WHAT IS BEING WRITTEN UP**: a bug report (Bust a bug, round 21) or a feature idea (Request a
/// feature, round 24). The CEO, 2026-10-11: *"Under the "Bust a bug" button, I want to add a
/// "Request a feature" button. But I don't want feature requests to be submitted or branded as
/// "issues" on GitHub."*, and *"can things not be copy-pasted from the "Bust a bug" feature?"*.
///
/// So an idea goes through ALL of this file exactly as a bug report does (the same scanner, Rich
/// writing it up and having the last word, the same card, the same decision at Send, the same
/// keeping and retrying of what could not go), and differs in two places only: the words Rich is
/// asked to write (an idea's sections, [`Post::sections`]), and where it is posted
/// ([`Outbox::attempt`]): a post in RichOS's GitHub Discussions under Ideas, through GraphQL's
/// `createDiscussion` ([`post_discussion`]), never the issues endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Post {
    #[default]
    Bug,
    Idea,
}

impl Post {
    /// What Rich's prompts call it.
    fn what(self) -> &'static str {
        match self {
            Post::Bug => "bug report",
            Post::Idea => "feature idea",
        }
    }
    /// Where it goes once the user approves it, as Rich's prompts say it.
    fn goes(self) -> &'static str {
        match self {
            Post::Bug => "filed as a GitHub issue that anyone can read",
            Post::Idea => "posted in RichOS's GitHub Discussions, under Ideas, where anyone can read it",
        }
    }
}

/// One field of Rich's write-up ([`Written`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    WhatHappened,
    Where,
    Steps,
    Expected,
}

impl Post {
    /// **EACH SECTION OF THE WRITE-UP, IN THE CARD'S ORDER**: its heading and the field of
    /// [`Written`] it holds. The first is always there; the others only when Rich wrote them. The
    /// version follows them all. An idea's are round 24's: *The idea*, *Why it would help*, *What
    /// the user pictured* and *Where*.
    fn sections(self) -> [(&'static str, Field); 4] {
        match self {
            Post::Bug => [("What happened", Field::WhatHappened), ("Where", Field::Where), ("Steps to see it", Field::Steps), ("What the user expected", Field::Expected)],
            Post::Idea => [("The idea", Field::WhatHappened), ("Why it would help", Field::Expected), (BULLETED, Field::Steps), ("Where", Field::Where)],
        }
    }
}

impl Written {
    fn text(&self, field: Field) -> &str {
        match field {
            Field::WhatHappened => &self.what_happened,
            Field::Where => &self.where_,
            Field::Expected => &self.expected,
            Field::Steps => "",
        }
    }
    /// The sections Rich wrote, laid out as [`Post::sections`] says: (heading, its text, its
    /// steps), the first always, the others when they have words.
    fn laid_out(&self) -> Vec<(&'static str, &str, &[String])> {
        self.post
            .sections()
            .iter()
            .enumerate()
            .filter_map(|(n, &(heading, field))| {
                let (text, steps): (&str, &[String]) = if field == Field::Steps { ("", &self.steps) } else { (self.text(field), &[]) };
                (n == 0 || !text.is_empty() || !steps.is_empty()).then_some((heading, text, steps))
            })
            .collect()
    }
}

/// The repository's Discussions category every idea is posted in (round 24; the CEO-input file
/// `docs/ceo-input/2026-10-11_01/request.md`: Discussions turned on, category "Ideas", slug `ideas`).
pub const IDEAS_CATEGORY_SLUG: &str = "ideas";

/// **THE SECTION OF AN IDEA WHOSE POINTS ARE A BULLETED LIST** (round 24: *What the user
/// pictured*, drawn as bullets). A bug report's steps are numbered; these points are not steps.
/// The card cannot change a heading, so the heading is what says how its list is drawn and written.
pub const BULLETED: &str = "What the user pictured";

// ---------------------------------------------------------------------------------------
// WHAT IS PRIVATE
// ---------------------------------------------------------------------------------------

/// The kinds of private detail round 21 leaves out, each with its stand-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    ConversationName,
    CompanyName,
    PersonName,
    FolderName,
    FilePath,
    EmailAddress,
    /// Anything else Rich names as private (a product, a client's project).
    PrivateWord,
}

impl Kind {
    /// Every kind there is.
    pub const ALL: [Kind; 7] =
        [Kind::ConversationName, Kind::CompanyName, Kind::PersonName, Kind::FolderName, Kind::FilePath, Kind::EmailAddress, Kind::PrivateWord];

    /// The words that go on GitHub in its place.
    pub fn stand_in(self) -> &'static str {
        match self {
            Kind::ConversationName => "[a conversation]",
            Kind::CompanyName => "[a company]",
            Kind::PersonName => "[a person]",
            Kind::FolderName => "[a folder]",
            Kind::FilePath => "[a file on this Mac]",
            Kind::EmailAddress => "[an email address]",
            Kind::PrivateWord => "[a private word]",
        }
    }
    /// One of it, in a sentence: "a conversation name".
    pub fn singular(self) -> &'static str {
        match self {
            Kind::ConversationName => "conversation name",
            Kind::CompanyName => "company name",
            Kind::PersonName => "person's name",
            Kind::FolderName => "folder name",
            Kind::FilePath => "file path",
            Kind::EmailAddress => "email address",
            Kind::PrivateWord => "private word",
        }
    }
    /// More than one: "conversation names".
    pub fn plural(self) -> &'static str {
        match self {
            Kind::ConversationName => "conversation names",
            Kind::CompanyName => "company names",
            Kind::PersonName => "people's names",
            Kind::FolderName => "folder names",
            Kind::FilePath => "file paths",
            Kind::EmailAddress => "email addresses",
            Kind::PrivateWord => "private words",
        }
    }
    /// The word Rich uses for a kind in his answer's `private` list.
    fn from_rich(word: &str) -> Kind {
        match word.trim().to_ascii_lowercase().as_str() {
            "person" | "people" | "name" => Kind::PersonName,
            "company" | "business" | "organization" => Kind::CompanyName,
            "conversation" | "thread" => Kind::ConversationName,
            "folder" | "repository" => Kind::FolderName,
            "file" | "path" => Kind::FilePath,
            "email" => Kind::EmailAddress,
            _ => Kind::PrivateWord,
        }
    }
}

/// One thing RichOS knows is private, and what kind it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivateTerm {
    pub text: String,
    pub kind: Kind,
}

impl PrivateTerm {
    pub fn new(text: &str, kind: Kind) -> Self {
        PrivateTerm { text: text.trim().to_string(), kind }
    }
}

/// A run of report text: plain words, or a stand-in (it has a `kind`), with what it replaced when
/// that is known.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub text: String,
    /// What the stand-in replaced. Shown to the user on the card; never part of the issue. `None`
    /// on a stand-in nothing accounts for: Rich wrote it himself and listed nothing beside it
    /// ([`finished`]), so the app does not know the words, and the card says only their kind.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub was: Option<String>,
    /// The kind of private detail a stand-in leaves out. `None` on plain words, and only there.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub kind: Option<Kind>,
}

impl Segment {
    fn plain(text: &str) -> Self {
        Segment { text: text.to_string(), was: None, kind: None }
    }
    fn stand_in(was: &str, kind: Kind) -> Self {
        Segment { text: kind.stand_in().to_string(), was: Some(was.to_string()), kind: Some(kind) }
    }
    /// A stand-in whose replaced words nothing accounts for: marked, counted and explained by its
    /// kind alone (nightly 48's walk, D23).
    fn unaccounted(kind: Kind) -> Self {
        Segment { text: kind.stand_in().to_string(), was: None, kind: Some(kind) }
    }
    /// **A STAND-IN, KNOWN OR NOT WHAT IT REPLACED**: the one test of whether a segment is one.
    /// On d0d06501c this was "it has `was`", so a stand-in Rich wrote himself was plain words: it
    /// was not underlined, and the line under the card said nothing private was in a report that
    /// carried four (D23).
    pub fn is_stand_in(&self) -> bool {
        self.kind.is_some()
    }
}

/// Replaces what is private with stand-ins.
///
/// THE MATCHING RULES, each one a trade stated rather than a default inherited:
///
///   - **A word Rich names as private** (his write-up's and his changes' `private` list,
///     [`Scrubber::with_rich`]) is matched in any letter case, however many words it is and
///     however it is written: "Zephyr" is a conversation's name when he says it is, and "secretco"
///     when he writes it so. The two rules below are guesses about names nobody has looked at,
///     and his word overrides them; where RichOS holds the same name in other capitals, his entry
///     is the one kept (review rv-20261009T172237Z-63b3f510-acf4 finding 1: on 63b3f5101 both
///     guesses left a name he had listed in the public report).
///   - A name of **two or more words** is matched in any letter case, in every alphabet ("acme
///     deal", "CAFÉ NORTH" for "Café North", [`fold_char`]).
///   - **An accented letter is one letter however it is encoded**: "é" as one character or as "e"
///     and a combining accent ([`folded`]). This holds for every rule here.
///   - A **one-word conversation name** is not matched at all (round 21 `privateTerms` does the
///     same): "Running" is a conversation in the demo and an ordinary word everywhere else.
///   - Any other **one-word name written Capitalized** ("Acme", "Deeply") is matched only where it
///     is written with a capital, so "deeply" in a sentence is left alone. One written in lower case
///     ("femcboost", a folder) is matched in any case, because it is not an ordinary word.
///   - Every match is a whole word: "Acmes" and "subAcme" are not "Acme".
///   - **Spacing is not part of a name**: any run of spaces, tabs or line breaks between its words
///     matches any run in the text, so "Jane\nDoe" listed is "Jane Doe" written, and the other way
///     round ([`needle_of`]). At least one is still needed: "JaneDoe" is one word.
///   - **File paths**: any word with a `/` or a `\` in it, except a web address (`https://…`, with no `\`),
///     "and/or", "w/", "w/o", "n/a", "24/7", and an all-digit fraction or date ("1/2", 10/09/2026).
///     The whole line it is on is left out, so no space, comma, bracket, quote or ". " in a
///     file's name, before the slash or after it, leaves a piece behind ([`find_paths`]).
///   - Where two of these overlap, both are left out as one.
///     **Email addresses** are `local@host.tld`.
#[derive(Debug, Clone, Default)]
pub struct Scrubber {
    /// The names RichOS holds, after the rules above dropped what they drop.
    terms: Vec<PrivateTerm>,
    /// The words Rich named as private, every one kept and matched in any letter case.
    rich: Vec<PrivateTerm>,
}

impl Scrubber {
    /// The names RichOS holds (`terms`), under the rules above.
    pub fn new(terms: Vec<PrivateTerm>) -> Self {
        Self::with_rich(terms, Vec::new())
    }

    /// The names RichOS holds (`app`), under the rules above, and the words Rich named as private
    /// (`rich`), every one of them, in any letter case. A name RichOS holds that Rich also named,
    /// in any capitals, is matched as his.
    pub fn with_rich(app: Vec<PrivateTerm>, rich: Vec<PrivateTerm>) -> Self {
        let mut rich = distinct(rich);
        let mut kept: Vec<PrivateTerm> = Vec::new();
        for term in app {
            let one_word = term.text.split_whitespace().count() < 2;
            if term.text.chars().count() < 2 || (one_word && term.kind == Kind::ConversationName) {
                continue;
            }
            if !kept.iter().chain(&rich).any(|k| key(&k.text) == key(&term.text)) {
                kept.push(term);
            }
        }
        // Longest first, so "Acme deal" wins over "Acme" where both match.
        kept.sort_by_key(|t| std::cmp::Reverse(t.text.len()));
        rich.sort_by_key(|t| std::cmp::Reverse(t.text.len()));
        Scrubber { terms: kept, rich }
    }

    /// The names RichOS holds in force, after the rules above dropped what they drop. Rich's own
    /// words are not among them: they are never put through those rules.
    pub fn terms(&self) -> &[PrivateTerm] {
        &self.terms
    }

    /// This scrubber with `rich` added to the words Rich named as private.
    fn and_rich(&self, rich: impl IntoIterator<Item = PrivateTerm>) -> Self {
        Self::with_rich(self.terms.clone(), self.rich.iter().cloned().chain(rich).collect())
    }

    /// Every private span in `text`, as (start, end, kind), non-overlapping and in order. Spans
    /// that overlap are joined into one covering both, with the kind of the one that starts first:
    /// dropping the later one would leave its uncovered part public ("Mary Jane" and "Jane Smith"
    /// in "Mary Jane Smith", or a company name that runs past the end of a path it starts in).
    fn spans(&self, text: &str) -> Vec<(usize, usize, Kind)> {
        let mut found: Vec<(usize, usize, Kind)> = Vec::new();
        // Rich's words first, so at the same span his kind is the one shown.
        let terms = self.rich.iter().map(|t| (t, true)).chain(self.terms.iter().map(|t| (t, false)));
        for (term, his) in terms {
            for (start, end) in find_term(text, &term.text, his) {
                found.push((start, end, term.kind));
            }
        }
        found.extend(find_paths(text).into_iter().map(|(s, e)| (s, e, Kind::FilePath)));
        found.extend(find_emails(text).into_iter().map(|(s, e)| (s, e, Kind::EmailAddress)));
        // Earliest first; at the same start, the longest.
        found.sort_by_key(|&(start, end, _)| (start, std::cmp::Reverse(end)));
        let mut out: Vec<(usize, usize, Kind)> = Vec::new();
        for span in found {
            match out.last_mut() {
                Some(last) if span.0 < last.1 => last.1 = last.1.max(span.1),
                _ => out.push(span),
            }
        }
        outside_stand_ins(text, out)
    }

    /// `text` as segments, with every private span replaced by its stand-in.
    pub fn scrub(&self, text: &str) -> Vec<Segment> {
        let mut out: Vec<Segment> = Vec::new();
        let mut at = 0;
        for (start, end, kind) in self.spans(text) {
            if start > at {
                out.push(Segment::plain(&text[at..start]));
            }
            out.push(Segment::stand_in(&text[start..end], kind));
            at = end;
        }
        if at < text.len() || out.is_empty() {
            out.push(Segment::plain(&text[at..]));
        }
        out
    }

    /// **AN ISSUE'S BODY, SCRUBBED AS ONE WHOLE STRING**: `markdown` as this module writes it
    /// ([`issue_body`]), with every private span of the words GitHub shows ([`shown`]) replaced by
    /// its stand-in, escaped like every other word of it. The scrub [`as_posted`] makes the card
    /// with reads the body exactly so, part by part ([`body_parts`]); a body written out from its
    /// card is left as it is by this, which is how a test can see that nothing was left behind.
    pub fn scrub_markdown(&self, markdown: &str) -> String {
        let (words, at) = shown(markdown);
        // Where the character shown at `offset` starts in `markdown`; the end of the words is the end of it.
        let start_of = |offset: usize| at.binary_search_by_key(&offset, |&(w, _)| w).map_or(markdown.len(), |k| at[k].1);
        let mut out = String::with_capacity(markdown.len());
        let mut last = 0;
        for (start, end, kind) in self.spans(&words) {
            out.push_str(&markdown[last..start_of(start)]);
            out.push_str(&escaped(kind.stand_in()));
            last = start_of(end);
        }
        out.push_str(&markdown[last..]);
        out
    }

    /// The private words still in `text`, as written: the card's heads-up after the user changes
    /// the report (*"Acme" looks private*).
    pub fn private_in(&self, text: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (start, end, _) in self.spans(text) {
            let word = text[start..end].to_string();
            if !out.contains(&word) {
                out.push(word);
            }
        }
        out
    }
}

/// **A letter or a digit, and nothing else**: what a private name must not run into on either side.
/// An underscore is not one: in Markdown it is emphasis, like `*` and `~`, so "_Jane Doe_" shows
/// "Jane Doe" in the issue (review rv-20261009T183159Z-a2fc94dc-20e7 finding 1: on a2fc94dc4 an
/// underscore counted as part of a word, the name was never whole there, and "\_Jane Doe\_" went
/// out with no heads-up, in the draft and after a change Rich checked).
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
}

/// Private words found in some text: where they start and end in it (bytes), and their kind.
type Span = (usize, usize, Kind);

/// **A STAND-IN IS NEVER PRIVATE**: `spans` (in order, not overlapping) with every stand-in
/// written in `text` taken out of them, and the spaces at either end of what is left. A stand-in
/// is the words that go on GitHub in a private word's place, so it is never left out itself: a
/// person named "Mac" leaves "[a file on this Mac]" as it reads, and a path on a line that starts
/// with "[a person]" leaves out the rest of the line and keeps the stand-in. This is what makes
/// the last scrub end: a report scrubbed again has nothing more to leave out ([`as_posted`]).
fn outside_stand_ins(text: &str, spans: Vec<Span>) -> Vec<Span> {
    let mut stand_ins: Vec<(usize, usize)> =
        Kind::ALL.iter().flat_map(|k| text.match_indices(k.stand_in()).map(|(at, s)| (at, at + s.len()))).collect();
    stand_ins.sort_unstable();
    let mut out = Vec::with_capacity(spans.len());
    let mut keep = |start: usize, end: usize, kind: Kind| {
        let words = &text[start..end];
        let start = start + (words.len() - words.trim_start().len());
        let end = start + words.trim().len();
        if start < end {
            out.push((start, end, kind));
        }
    };
    for (start, end, kind) in spans {
        let mut at = start;
        for &(s, e) in stand_ins.iter().filter(|&&(s, e)| s < end && e > start) {
            if s > at {
                keep(at, s, kind);
            }
            at = at.max(e);
        }
        if at < end {
            keep(at, end, kind);
        }
    }
    out
}

fn char_before(text: &str, at: usize) -> Option<char> {
    text[..at].chars().next_back()
}

fn char_after(text: &str, at: usize) -> Option<char> {
    text[at..].chars().next()
}

/// One character with its letter case folded away, in every alphabet: lower case, then upper,
/// then lower again, so "É" and "é" are one letter, "Ë" and "ë", "Σ", "σ" and "ς", and "ß", "ẞ"
/// and "SS" ("STRASSE" is "Straße"). Unicode's full case folding, built from the standard
/// library's own case tables rather than a second table kept here (fourth review finding 1:
/// ASCII-only folding left "CAFÉ NORTH" in a public report for the private name "Café North").
fn fold_char(c: char) -> impl Iterator<Item = char> {
    c.to_lowercase().flat_map(char::to_uppercase).flat_map(char::to_lowercase)
}

/// `text` with its letter case folded away ([`fold_char`]) and its accents written one way: the
/// same words in any capitals, and however their accented letters are encoded, fold to the same
/// string. "é" is one character (U+00E9) or "e" followed by a combining acute accent (U+0301),
/// the same visible letter; both are compared decomposed, Unicode's canonical caseless match
/// (D145: decompose, fold, decompose again, since folding can make a letter that decomposes).
/// Review rv-20261009T184833Z-c3c40906-bf02 finding 1: on c3c40906d, with Rich naming "Café
/// North" private one way, "CAFÉ NORTH" written the other way went out with no heads-up.
fn folded(text: &str) -> String {
    let once: String = text.nfd().flat_map(fold_char).collect();
    once.nfd().collect()
}

/// **The same words, however they are spaced**: `text` with its letter case folded away
/// ([`folded`]) and every run of spaces, tabs and line breaks as one space, ends trimmed. Two
/// private words with one key are one word ([`distinct`], [`Scrubber::with_rich`]).
fn key(text: &str) -> String {
    text.split_whitespace().map(folded).collect::<Vec<_>>().join(" ")
}

/// What a match of `term` must hold, one folded character at a time ([`folded`]), with `None`
/// for each run of spaces, tabs and line breaks between its words: any run in the term matches
/// any run in the text, so "Jane\nDoe", "Jane  Doe" and "Jane Doe" are one name whichever of them
/// Rich listed and whichever the text holds (review rv-20261009T173448Z-baf60f3c-c54d finding 1:
/// on baf60f3c0 the draft joined a line before scrubbing it, Rich's "Jane\nDoe" kept its line
/// break, and "Jane Doe" went public with no heads-up). Spaces before the first word and after
/// the last are not part of it.
fn needle_of(term: &str) -> Vec<Option<char>> {
    let mut out = Vec::new();
    for word in term.split_whitespace() {
        if !out.is_empty() {
            out.push(None);
        }
        out.extend(folded(word).chars().map(Some));
    }
    out
}

/// Where a match of `needle` ([`needle_of`]) that starts at byte `start` of `text` ends, if one
/// does: `text` is folded ([`folded`]) one letter as written at a time, a character with the
/// combining marks that follow it, so the end is always the end of a whole letter of the ORIGINAL
/// text, whatever lengths the two spellings have ("ß" is one character and folds to two; "é" is
/// one character or two, and decomposes to two), and an accent is never left behind outside a
/// match nor a name found inside a letter ("Cafe" is not in "Café" written either way). A `None`
/// in the needle takes a whole run of whitespace in the text, one character or many; the needle
/// neither starts nor ends with one, so a match starts and ends on a word.
fn folded_match_end(text: &str, start: usize, needle: &[Option<char>]) -> Option<usize> {
    let rest = &text[start..];
    let mut matched = 0;
    let mut in_space = false;
    let mut chars = rest.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c.is_whitespace() {
            if !in_space {
                if needle.get(matched) != Some(&None) {
                    return None;
                }
                matched += 1;
                in_space = true;
            }
            continue;
        }
        in_space = false;
        let mut end = i + c.len_utf8();
        while let Some(&(j, mark)) = chars.peek() {
            if canonical_combining_class(mark) == 0 {
                break;
            }
            end = j + mark.len_utf8();
            chars.next();
        }
        for f in folded(&rest[i..end]).chars() {
            if needle.get(matched) != Some(&Some(f)) {
                return None;
            }
            matched += 1;
        }
        if matched == needle.len() {
            return Some(start + end);
        }
    }
    None
}

/// Whole-word, letter-case-aware occurrences of `term` in `text` (see [`Scrubber`]'s rules).
/// Letter case is compared in every alphabet ([`fold_char`]); every offset is a character
/// boundary of `text`, so the stand-in replaces exactly the words as the user wrote them.
/// `his`: Rich named it private, so it is matched in any letter case, one word or several.
fn find_term(text: &str, term: &str, his: bool) -> Vec<(usize, usize)> {
    let needle = needle_of(term);
    let one_word = !his && !needle.contains(&None);
    let capitalized = term.trim_start().chars().next().is_some_and(char::is_uppercase);
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    let mut skip_to = 0;
    for (i, _) in text.char_indices() {
        if i < skip_to {
            continue;
        }
        let Some(end) = folded_match_end(text, i, &needle) else { continue };
        let bounded = !char_before(text, i).is_some_and(is_word_char) && !char_after(text, end).is_some_and(is_word_char);
        let case_ok = !(one_word && capitalized) || char_after(text, i).is_some_and(char::is_uppercase);
        if bounded && case_ok {
            out.push((i, end));
            skip_to = end;
        }
    }
    out
}

/// **The end of the line** a path starts at `from`: the first line break, or the end of the text.
/// A file's name can hold spaces, commas, semicolons, brackets, quotes and a full stop followed by
/// a space ("Smith, Jones Budget.xlsx", "Budget (draft) Client.xlsx", "Dr. SecretClient
/// Budget.xlsx"), so none of them says where it ends, and the words between the path and the end
/// of its line go with it (review rv-20261009T151254Z-84d1bdce-20df finding 1: a comma ended it;
/// review rv-20261009T154959Z-96dcc581-085b finding 1: a closing bracket did; cc/echo-opus-bug7
/// at 1e7b696cf: ". " did, so "Dr. SecretClient Budget.xlsx" left "SecretClient Budget.xlsx"
/// public).
fn line_end(text: &str, from: usize) -> usize {
    text[from..].find(['\n', '\r']).map_or(text.len(), |i| from + i)
}

/// The short fixed list of words with a slash that are not paths ("w/" is "with").
const SLASH_WORDS: [&str; 5] = ["and/or", "w/", "w/o", "n/a", "24/7"];

/// **A word with a slash that is not a path**, and nothing else is: a web address (`http://`,
/// `https://`), a word on [`SLASH_WORDS`] in any capitals, or all digits on each side of its
/// slashes, one to four of them: a fraction ("1/2") or a date ("10/09", "10/09/2026"). Two plain
/// words ("Clients/SecretCo") are a path (cc/echo-opus-bug7 at 1e7b696cf left them public as a
/// short form). Brackets, quotes and punctuation around the word are not part of the form
/// ("(and/or)", "24/7,"). A backslash never makes one, a web address included
/// ("https://example.org\SecretCo\budget.xlsx" is a path): there is no common form written with it
/// (review rv-20261009T162841Z-69294215-70e6 finding 3: the web address was checked first).
fn is_not_a_path(word: &str) -> bool {
    if word.contains('\\') {
        return false;
    }
    let core = word.trim_matches(|c: char| !(c.is_alphanumeric() || c == '/'));
    let head = word.trim_start_matches(|c: char| !c.is_alphanumeric());
    let web = ["http://", "https://"].iter().any(|s| head.get(..s.len()).is_some_and(|h| h.eq_ignore_ascii_case(s)));
    if web {
        return true;
    }
    if SLASH_WORDS.iter().any(|w| core.eq_ignore_ascii_case(w)) {
        return true;
    }
    let parts: Vec<&str> = core.split('/').collect();
    let number = |p: &&str| (1..=4).contains(&p.len()) && p.chars().all(|c| c.is_ascii_digit());
    (2..=3).contains(&parts.len()) && parts.iter().all(number)
}

/// **The start of the line** the word at `at` is on: just after the line break before it, past
/// the line's leading spaces.
fn line_start(text: &str, at: usize) -> usize {
    let start = text[..at].rfind(['\n', '\r']).map_or(0, |i| i + 1);
    at - text[start..at].trim_start().len()
}

/// **File paths**: every word (a run of text between spaces) with a `/` or a `\` in it, except
/// the ones [`is_not_a_path`] names. **The whole line it is on is left out**, from the line's
/// first word ([`line_start`]) to its end ([`line_end`]); the line's own final full stop, colon,
/// "!" or "?" stays outside.
///
/// The rule recognizes no shape of path (review rv-20261009T154959Z-96dcc581-085b: three rounds
/// in a row each found one more shape that leaked, the last nested brackets and relative paths).
/// `/Users/…`, `~/…`, `./…`, `../…`, `clients/x/y.xlsx`, `C:\…`, `file://…` and a path between
/// brackets that hold brackets are all a word with a slash in it. Nothing in the words says where
/// a file's name starts or ends: a name can hold spaces and every mark, so the words before the
/// slash can be the start of it ("SecretCo Plans/budget.xlsx" left "SecretCo" public, review
/// rv-20261009T162841Z-69294215-70e6 finding 1), and the words after it the end. Hiding the rest
/// of a line is safe; leaving part of a file's name in a public report is not.
fn find_paths(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut skip_to = 0;
    for word in text.split_whitespace() {
        // `split_whitespace` gives slices of `text` itself, so where the word sits in memory is
        // where it starts in `text`: a character boundary, since a word starts after a space.
        let at = word.as_ptr() as usize - text.as_ptr() as usize;
        if at < skip_to || !word.contains(['/', '\\']) || is_not_a_path(word) {
            continue;
        }
        let start = line_start(text, at);
        let mut end = at + text[at..line_end(text, at)].trim_end().len();
        while end > start && text[..end].ends_with(['.', ':', '!', '?']) {
            end -= 1;
        }
        if end > start {
            out.push((start, end));
            skip_to = end;
        }
    }
    out
}

/// Email addresses: `local@host.tld`. Every offset is a character boundary: the address may
/// follow an emoji or an accented letter.
fn find_emails(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let local = |c: char| c.is_alphanumeric() || "._%+-".contains(c);
    let host = |c: char| c.is_alphanumeric() || ".-".contains(c);
    for (at, _) in text.match_indices('@') {
        let start = text[..at].char_indices().rev().find(|&(_, c)| !local(c)).map(|(p, c)| p + c.len_utf8()).unwrap_or(0);
        let mut end = at + 1 + text[at + 1..].find(|c: char| !host(c)).unwrap_or(text.len() - at - 1);
        while end > at + 1 && text[..end].ends_with(['.', '-']) {
            end -= 1;
        }
        let domain = &text[at + 1..end];
        let dotted = domain.rfind('.').is_some_and(|d| d > 0 && d + 1 < domain.len());
        if start < at && dotted && out.last().is_none_or(|&(_, e)| start >= e) {
            out.push((start, end));
        }
    }
    out
}

/// The line under the sheet (round 21): what was left out, counted by kind, in the order first
/// met. One stand-in may stand for several names ("Acme deal and Partner book review") when Rich
/// wrote them together; each counts.
pub fn left_out_line(segments: &[Segment]) -> String {
    let mut order: Vec<Kind> = Vec::new();
    let mut counts: std::collections::HashMap<Kind, usize> = std::collections::HashMap::new();
    for segment in segments {
        let Some(kind) = segment.kind else { continue };
        if !counts.contains_key(&kind) {
            order.push(kind);
        }
        *counts.entry(kind).or_default() += 1;
    }
    if order.is_empty() {
        return "Nothing private was in it, so nothing was left out. Anyone can read GitHub issues.".into();
    }
    let parts: Vec<String> = order
        .iter()
        .map(|kind| {
            let n = counts[kind];
            format!("{} {}", number_word(n), if n == 1 { kind.singular() } else { kind.plural() })
        })
        .collect();
    let list = match parts.len() {
        1 => parts[0].clone(),
        _ => format!("{} and {}", parts[..parts.len() - 1].join(", "), parts[parts.len() - 1]),
    };
    let total: usize = counts.values().sum();
    let pronoun = if total == 1 { "it" } else { "them" };
    format!(
        "Left out, because anyone can read GitHub issues: {list}. The underlined words stand in for {pronoun}; point at one to see what it replaced."
    )
}

fn number_word(n: usize) -> String {
    const WORDS: [&str; 10] = ["no", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine"];
    WORDS.get(n).map(|w| w.to_string()).unwrap_or_else(|| n.to_string())
}

// ---------------------------------------------------------------------------------------
// WHERE THE USER WAS
// ---------------------------------------------------------------------------------------

/// The screen the user was on when they pressed Bust a bug, as the UI read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Screen {
    /// `conversation`, `corrections`, `feedback`, `search`, `picker`, `unbound`, `company`,
    /// `home`, `output` or `other`.
    pub key: String,
    /// In the user's own terms, private names included: "the Acme deal conversation".
    pub here: String,
    /// The same place described for the public report, with no names: "a conversation, the
    /// main RichOS screen".
    pub public: String,
    /// The conversation on screen, when there is one.
    #[serde(default)]
    pub conversation: Option<String>,
    /// Text size, in percent (Settings → Text size).
    #[serde(default = "hundred")]
    pub text_size: u32,
    /// `light` or `dark`, as painted.
    #[serde(default)]
    pub theme: String,
    #[serde(default)]
    pub technical_view: bool,
    /// **WHAT WAS ON IT**: the screen's visible words, top to bottom, as the window read them the
    /// moment Bust a bug was pressed, before the exchange covered anything. Private names
    /// included: it goes to the user's own Claude to check against, never into the issue.
    #[serde(default)]
    pub content: String,
}

/// The most of the screen's words Rich is given, in characters: enough for a full window of a
/// conversation, and a bound so a long one cannot crowd out the user's own words.
pub const SCREEN_CONTENT_MAX: usize = 12_000;

fn hundred() -> u32 {
    100
}

/// "RichOS 1.2.0, nightly 45 · macOS 15.6 · Apple silicon", from the bundle's own version.
pub fn version_line(app_version: &str, macos: Option<&str>, arch: &str) -> String {
    let (base, pre) = app_version.split_once('-').unwrap_or((app_version, ""));
    let mut parts = vec![if let Some(rest) = pre.strip_prefix("nightly.") {
        match rest.rsplit_once('.') {
            Some((_, n)) => format!("RichOS {base}, nightly {n}"),
            None => format!("RichOS {base}, nightly {rest}"),
        }
    } else if let Some(sha) = pre.strip_prefix("dev.") {
        format!("RichOS {base}, development build {sha}")
    } else if pre.is_empty() {
        format!("RichOS {base}")
    } else {
        format!("RichOS {app_version}")
    }];
    if let Some(v) = macos.map(str::trim).filter(|v| !v.is_empty()) {
        parts.push(format!("macOS {v}"));
    }
    parts.push(match arch {
        "aarch64" => "Apple silicon".to_string(),
        "x86_64" => "Intel".to_string(),
        other => other.to_string(),
    });
    parts.join(" · ")
}

// ---------------------------------------------------------------------------------------
// RICH'S WRITE-UP
// ---------------------------------------------------------------------------------------

/// What Rich wrote, before anything private is replaced.
///
/// **A FEATURE IDEA USES THE SAME FIELDS** ([`Post::sections`] names each one's heading): its
/// `what_happened` is *The idea*, `expected` is *Why it would help*, `steps` are the points of
/// *What the user pictured* (a bulleted list, [`BULLETED`]) and `where_` is *Where*.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Written {
    /// A bug report or a feature idea.
    pub post: Post,
    pub title: String,
    pub what_happened: String,
    pub where_: String,
    pub steps: Vec<String>,
    pub expected: String,
    /// Further private words Rich found in what he wrote, beyond the ones RichOS holds.
    pub private: Vec<PrivateTerm>,
    /// What Rich saw on the screen that bears the user out, in a few words ("Saw the names cut
    /// off at 135% text size"), or empty. Shown to the user over his answer (round 21's
    /// digest), never in the issue.
    pub checked: String,
}

/// The arguments `claude` is run with to write the report: one printed answer, no tools, none
/// of the operator's settings, no session left on disk, no MCP servers. The account comes from
/// `CLAUDE_CONFIG_DIR`, which the shell sets to the account the conversation runs on. No
/// `--model`: Rich writes with the same model he talks with.
///
/// **The prompt arrives as one stream-json user message on standard input** ([`writer_input`]),
/// because that is how a picture reaches him: an image block beside the words. Measured with
/// claude 2.1.295 on 2026-10-09 with exactly these arguments: a 64 by 64 red picture and "What
/// single color fills the attached picture?" came back `{"color": "red"}` on the `result` line.
/// stream-json output needs `--verbose`; [`result_text`] reads the last `result` line.
pub fn writer_args() -> Vec<String> {
    [
        "--print",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--setting-sources",
        "",
        "--no-session-persistence",
        "--tools",
        "",
        "--strict-mcp-config",
        "--mcp-config",
        "{\"mcpServers\":{}}",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn terms_line(terms: &[PrivateTerm]) -> String {
    let mut words: Vec<&str> = terms.iter().map(|t| t.text.as_str()).collect();
    words.sort_unstable();
    words.dedup();
    if words.is_empty() {
        "none".into()
    } else {
        words.join("; ")
    }
}

fn screen_lines(screen: &Screen, version: &str) -> String {
    let mut lines = vec![
        format!("- The screen: {}", screen.here),
        format!("- The same place described with no names, for the public report: {}", screen.public),
        format!("- Text size: {}%", screen.text_size),
    ];
    if !screen.theme.is_empty() {
        lines.push(format!("- Theme: {}", screen.theme));
    }
    lines.push(format!("- Technical view: {}", if screen.technical_view { "on" } else { "off" }));
    lines.push(format!("- Version: {version}"));
    lines.join("\n")
}

/// The screen's words as Rich is given them: each line's whitespace folded, empty lines dropped,
/// at most [`SCREEN_CONTENT_MAX`] characters, cut on a character boundary.
pub fn screen_words(content: &str) -> String {
    let lines: Vec<String> = content.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|l| !l.is_empty()).collect();
    let text = lines.join("\n");
    match text.char_indices().nth(SCREEN_CONTENT_MAX) {
        Some((cut, _)) => format!("{}\n[the rest of the screen is left off]", &text[..cut]),
        None => text,
    }
}


/// The one prompt Rich writes the report from. `picture` says a picture of the window goes with
/// it ([`writer_input`]).
pub fn writer_prompt(answer: &str, screen: &Screen, version: &str, terms: &[PrivateTerm], picture: bool) -> String {
    writer_prompt_for(Post::Bug, answer, screen, version, terms, picture)

}

/// The one prompt Rich writes a bug report or a feature idea from. A bug report's is
/// [`writer_prompt`]; an idea's asks for round 24's sections (*The idea*, *Why it would help*,
/// *What the user pictured*, *Where*), with the same rules for what is private.
pub fn writer_prompt_for(post: Post, answer: &str, screen: &Screen, version: &str, terms: &[PrivateTerm], picture: bool) -> String {
    let schema = match post {
        Post::Bug => serde_json::json!({"title":"string","what_happened":"string","where":"string","steps":["string"],"expected":"string","checked":"string","private":[{"text":"string","kind":"person|company|conversation|folder|file|email|other"}]}),
        Post::Idea => serde_json::json!({"title":"string","idea":"string","why":"string","pictured":["string"],"where":"string","checked":"string","private":[{"text":"string","kind":"person|company|conversation|folder|file|email|other"}]}),
    };
    serde_json::json!({"event":"user_requested_public_draft","kind":post.what(),"destination":post.goes(),"user_message":answer.trim(),"screen":screen_lines(screen,version),"screen_content":screen_words(&screen.content),"picture_attached":picture,"private_terms":terms_line(terms),"privacy_placeholders":stand_ins_line(),"output_schema":schema}).to_string()

}

/// Each kind Rich names a private word with, and the stand-in written in its place
/// ([`Kind::stand_in`]), as his prompts list them: "person [a person], company [a company], …".
fn stand_ins_line() -> String {
    [("person", Kind::PersonName), ("company", Kind::CompanyName), ("conversation", Kind::ConversationName), ("folder", Kind::FolderName), ("file", Kind::FilePath), ("email", Kind::EmailAddress), ("other", Kind::PrivateWord)]
        .iter()
        .map(|(word, kind)| format!("{word} {}", kind.stand_in()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A picture for Rich: its media type (`image/jpeg`) and its bytes in standard base64.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub media_type: String,
    pub base64: String,
}

/// **The one line `claude` reads on standard input** ([`writer_args`]): a stream-json user
/// message holding the picture, when there is one, and the prompt.
pub fn writer_input(prompt: &str, picture: Option<&Picture>) -> String {
    let mut content = Vec::new();
    if let Some(p) = picture {
        content.push(serde_json::json!({ "type": "image", "source": { "type": "base64", "media_type": p.media_type, "data": p.base64 } }));
    }
    content.push(serde_json::json!({ "type": "text", "text": prompt }));
    format!("{}\n", serde_json::json!({ "type": "user", "message": { "role": "user", "content": content } }))
}

/// **The line over Rich's answer** (round 21's digest): what he did. "Looked at the screen you
/// were on" (or at the window's name) only when he was given what was on the screen (`looked`);
/// then what he saw that bears the user out, when he says; then the version. There is no digest
/// without Rich: a report he did not write is not shown ([`checked`]).
pub fn digest(screen: &Screen, looked: bool, checked: &str) -> String {
    let place = match screen.key.as_str() {
        "corrections" | "feedback" | "search" => screen.here.clone(),
        _ => "the screen you were on".to_string(),
    };
    let mut first = format!("{} {place}", if looked { "Looked at" } else { "Noted" });
    if screen.text_size != 100 {
        first.push_str(&format!(", at {}% text size", screen.text_size));
    }
    let mut parts = vec![first];
    let checked = checked.trim().trim_end_matches('.');
    if looked && !checked.is_empty() && checked.chars().count() <= 80 {
        parts.push(capitalized(checked));
    }
    parts.push("Checked the version".into());
    parts.join(" · ")
}

/// The prompt for a change the user TELLS Rich (round 21: *"also say it happens in the light
/// theme too"*): he answers with the sentence to add and where it goes.
pub fn change_prompt(sheet: &Sheet, said: &str) -> String {
    change_prompt_for(Post::Bug, sheet, said)
}

/// [`change_prompt`] for a bug report or a feature idea: the same answer, about the post it is.
pub fn change_prompt_for(post: Post, sheet: &Sheet, said: &str) -> String {
    serde_json::json!({"event":"user_requested_draft_change","kind":post.what(),"destination":post.goes(),"draft":{"title":sheet.title,"body":issue_body(sheet)},"user_message":said.trim(),"section_headings":sheet.sections.iter().map(|section|section.heading.as_str()).collect::<Vec<_>>(),"output_schema":{"section":"string","add":"string","private":[{"text":"string","kind":"person|company|conversation|folder|file|email|other"}]}}).to_string()

}

/// The `result` text out of `claude --print`'s answer: the last `"type": "result"` line of its
/// stream-json output (one JSON object per line), or the one object `--output-format json`
/// prints. An error reply (not signed in, out of quota) is refused rather than read as a report.
pub fn result_text(stdout: &str) -> Result<String, String> {
    let value: serde_json::Value = stdout
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .find(|v| v["type"] == "result")
        .ok_or_else(|| "not a reply: no result line".to_string())?;
    if value["is_error"].as_bool() == Some(true) {
        return Err(format!("Claude answered with an error: {}", value["result"].as_str().unwrap_or("")));
    }
    value["result"].as_str().map(str::to_string).ok_or_else(|| "the reply had no result".to_string())
}

/// The first `{` to the last `}`: a model asked for only JSON sometimes fences it or says a word
/// first, and neither changes what it wrote.
fn json_object(raw: &str) -> Result<serde_json::Value, String> {
    let start = raw.find('{').ok_or("no JSON object in the answer")?;
    let end = raw.rfind('}').ok_or("no JSON object in the answer")?;
    if end < start {
        return Err("no JSON object in the answer".into());
    }
    serde_json::from_str(&raw[start..=end]).map_err(|e| format!("the answer's JSON did not read: {e}"))
}

fn text_of(value: &serde_json::Value, key: &str) -> String {
    value[key].as_str().unwrap_or("").trim().to_string()
}

/// **THE `private` LIST OF RICH'S ANSWER, OR NO ANSWER**, each term in the kind he named. The
/// list must be there and be a list, and every entry an object with a string `text` and a string
/// `kind`; `[]` is "nothing private" only when he wrote `[]`. Anything else is a malformed answer,
/// refused like a Claude failure (not offered, kept, asked again), never read as "nothing private":
/// on c0a24290f a list left out, given as a string, or with entries saying `name` instead of
/// `text` put "Jane Doe at SecretCo" on a card offered for sending (review
/// rv-20261009T171105Z-c0a24290-58b4 finding 1). An entry whose text is blank names nothing and
/// is skipped.
fn private_of(value: &serde_json::Value) -> Result<Vec<PrivateTerm>, String> {
    let list = match value.get("private") {
        None => return Err("the answer had no private list".into()),
        Some(serde_json::Value::Array(list)) => list,
        Some(_) => return Err("the answer's private list was not a list".into()),
    };
    let mut terms = Vec::new();
    for (n, entry) in list.iter().enumerate() {
        let (Some(text), Some(kind)) = (entry.get("text").and_then(|t| t.as_str()), entry.get("kind").and_then(|k| k.as_str())) else {
            return Err(format!("entry {} of the answer's private list had no text or no kind", n + 1));
        };
        if !text.trim().is_empty() {
            terms.push(PrivateTerm::new(text, Kind::from_rich(kind)));
        }
    }
    Ok(terms)
}

/// `terms` without repeats (the same words in any letter case, however spaced, are one term
/// ([`key`]); the first kind wins).
fn distinct(terms: impl IntoIterator<Item = PrivateTerm>) -> Vec<PrivateTerm> {
    let mut out: Vec<PrivateTerm> = Vec::new();
    for term in terms {
        if !key(&term.text).is_empty() && !out.iter().any(|t| key(&t.text) == key(&term.text)) {
            out.push(term);
        }
    }
    out
}

/// Rich's answer, read. A report needs a title and something that happened; anything less is
/// refused rather than shown hollow, and the report waits for him to answer again ([`checked`]).
pub fn parse_written(raw: &str) -> Result<Written, String> {
    parse_written_for(Post::Bug, raw)
}

/// Rich's answer about a bug report or a feature idea, read: the keys [`writer_prompt_for`] asked
/// him for. An idea needs a title and the idea itself; anything less is refused, as for a report.
pub fn parse_written_for(post: Post, raw: &str) -> Result<Written, String> {
    let value = json_object(raw)?;
    let list = |key: &str| -> Vec<String> {
        value[key].as_array().map(|a| a.iter().filter_map(|s| s.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default()
    };
    let (what, expected, steps) = match post {
        Post::Bug => ("what_happened", "expected", "steps"),
        Post::Idea => ("idea", "why", "pictured"),
    };
    let written = Written {
        post,
        title: text_of(&value, "title"),
        what_happened: text_of(&value, what),
        where_: text_of(&value, "where"),
        steps: list(steps),
        expected: text_of(&value, expected),
        private: private_of(&value)?,
        checked: text_of(&value, "checked"),
    };
    if written.title.is_empty() || written.what_happened.is_empty() {
        return Err(match post {
            Post::Bug => "the answer had no title or no account of what happened",
            Post::Idea => "the answer had no title or no idea",
        }
        .into());
    }
    Ok(written)
}

fn capitalized(text: &str) -> String {
    let text = text.trim();
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// **A REPORT RICH HAS CHECKED**: the card's draft, and the line over his answer ([`digest`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checked {
    pub draft: Draft,
    pub digest: String,
}

/// **RICH'S CHECK, OR NO REPORT TO SEND** (CEO §115: *"let their Rich check and articulate
/// everything properly and then submit"*). `rich` is his write-up, or why there is none; read
/// ([`parse_written`]), put through the scanner ([`draft_from`]), and given back to him LAST to
/// clean up what is still private (`finish`, one more turn of his, asked [`finish_prompt`] and read
/// by [`finished`]). His last words are the card's draft. When any of it cannot be had, there is
/// no draft: the plain write-up that stood in for him when Claude failed copied the user's words
/// into a public issue ("Jane Doe at SecretCo saw the window freeze", review
/// rv-20261009T162841Z-69294215-70e6 finding 2). `looked`: he was given what was on the screen.
pub fn checked(
    rich: Result<String, String>,
    screen: &Screen,
    looked: bool,
    version: &str,
    scrubber: &Scrubber,
    finish: impl FnOnce(&str) -> Result<String, String>,
) -> Result<Checked, String> {
    checked_for(Post::Bug, rich, screen, looked, version, scrubber, finish)
}

/// [`checked`] for a bug report or a feature idea: the same check, the same scanner, Rich last.
pub fn checked_for(
    post: Post,
    rich: Result<String, String>,
    screen: &Screen,
    looked: bool,
    version: &str,
    scrubber: &Scrubber,
    finish: impl FnOnce(&str) -> Result<String, String>,
) -> Result<Checked, String> {
    let written = parse_written_for(post, &rich?)?;
    let scanned = draft_from(&written, version, scrubber);
    let draft = rich_last(post, &scanned, &unscanned(&written, version), scrubber.terms(), finish)?;
    Ok(Checked { digest: digest(screen, looked, &written.checked), draft })
}

/// `scanned` (what the scanner made of a report) given to Rich last ([`finish_prompt`]) beside
/// `before`, the same report as it was before the scanner, with `app`, the names RichOS holds; and
/// read back as the card's draft ([`finished`]).
fn rich_last(post: Post, scanned: &Draft, before: &Sheet, app: &[PrivateTerm], finish: impl FnOnce(&str) -> Result<String, String>) -> Result<Draft, String> {
    let known: Vec<PrivateTerm> = app.iter().chain(&scanned.private).cloned().collect();
    finished(finish(&finish_prompt_for(post, &sheet_of(scanned), before, &known)), scanned)
}

/// A paragraph's or a step's words as they were before the scanner: split at blank lines into
/// paragraphs and each line break read as a space, as [`paragraphs_of`] does, with nothing left out.
fn paragraphs_as_written(text: &str) -> Vec<String> {
    text.split("\n\n").map(|p| p.replace('\n', " ").trim().to_string()).filter(|p| !p.is_empty()).collect()
}

/// **RICH'S WRITE-UP AS IT WAS BEFORE THE SCANNER**, laid out as [`draft_from`] lays out the
/// scanner's: the same sections in the same order, under the same headings, with nothing left out.
/// The title's one substitution ([`as_posted`]) is made here too, since it is how GitHub shows a
/// title, not something the scanner hid.
fn unscanned(written: &Written, version: &str) -> Sheet {
    let section = |heading: &str, paragraphs: Vec<String>, steps: Vec<String>| SheetSection { heading: heading.into(), paragraphs, steps };
    let mut sections: Vec<SheetSection> =
        written.laid_out().into_iter().map(|(heading, text, steps)| section(heading, paragraphs_as_written(text), steps.to_vec())).collect();
    sections.push(section("Version", vec![version.to_string()], vec![]));
    Sheet { title: written.title.trim().replace('`', "'"), sections }
}

/// **THE CARD'S WORDS AS THEY WERE BEFORE THE SCANNER** ([`redraft`]): laid out as `redraft` lays
/// them out, with nothing left out. A stand-in already on the card is its words here too.
fn as_typed(sheet: &Sheet) -> Sheet {
    Sheet {
        title: sheet.title.trim().replace('`', "'"),
        sections: sheet
            .sections
            .iter()
            .map(|s| SheetSection {
                heading: s.heading.trim().to_string(),
                paragraphs: paragraphs_as_written(&s.paragraphs.join("\n\n")),
                steps: s.steps.iter().map(|t| t.trim()).filter(|t| !t.is_empty()).map(str::to_string).collect(),
            })
            .collect(),
    }
}

/// A report as the JSON Rich's last pass reads and answers in.
fn report_json(sheet: &Sheet) -> String {
    let report = serde_json::json!({
        "title": sheet.title,
        "sections": sheet.sections.iter().map(|s| serde_json::json!({ "heading": s.heading, "paragraphs": s.paragraphs, "steps": s.steps })).collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&report).unwrap_or_default()
}

/// **THE PROMPT FOR RICH'S LAST PASS** (CEO §115: *"just let's the user's Rich check and clean-up
/// what needs to be cleaned-up in that particular user's case"*): the report as the scanner left
/// it (`sheet`), and the same report as it was before the scanner (`before`), both as JSON. He has
/// the final say: he puts back any words the scanner hid that are not private in this user's case
/// (review rv-20261009T204435Z-593f4791-c2ff finding 1: on 593f4791b he was told to keep every
/// stand-in, so "The Send/Cancel buttons overlap." went to the card as "[a file on this Mac]."),
/// and cleans up every private detail still there, writing it as the stand-in for its kind; he
/// changes nothing else. `known` are the private words RichOS holds and the ones he named before.
pub fn finish_prompt(sheet: &Sheet, before: &Sheet, known: &[PrivateTerm]) -> String {
    finish_prompt_for(Post::Bug, sheet, before, known)
}

/// [`finish_prompt`] for a bug report or a feature idea: the same last pass, about the post it is.
pub fn finish_prompt_for(post: Post, sheet: &Sheet, before: &Sheet, known: &[PrivateTerm]) -> String {
    serde_json::json!({"event":"public_draft_privacy_check","kind":post.what(),"destination":post.goes(),"draft":serde_json::from_str::<serde_json::Value>(&report_json(sheet)).expect("serialized draft"),"before_automatic_check":serde_json::from_str::<serde_json::Value>(&report_json(before)).expect("serialized draft"),"private_terms":terms_line(known),"privacy_placeholders":stand_ins_line(),"output_schema":{"title":"string","sections":[{"heading":"string","paragraphs":["string"],"steps":["string"]}],"private":[{"text":"string","kind":"person|company|conversation|folder|file|email|other"}]}}).to_string()

}

/// **THE WORDS OF ONE LIST OF A SECTION IN RICH'S ANSWER, OR NO ANSWER**: `key` (`paragraphs` or
/// `steps`) of section `n`, each entry trimmed, the blank ones dropped. A list that is there must
/// be a list of words; a list left out is no words. When `had` (the scanner's section had words
/// there) and none are left, the section's text is missing. Any of it is a malformed answer
/// (review rv-20261009T204435Z-593f4791-c2ff finding 2: on 593f4791b a paragraph list given as a
/// string became an empty list, and an empty report was offered for sending).
fn words_of(section: &serde_json::Map<String, serde_json::Value>, key: &str, n: usize, had: bool) -> Result<Vec<String>, String> {
    let words: Vec<String> = match section.get(key) {
        None => Vec::new(),
        Some(serde_json::Value::Array(list)) => {
            let mut words = Vec::new();
            for entry in list {
                let text = entry.as_str().ok_or_else(|| format!("section {n} of the answer had {key} that were not words"))?;
                if !text.trim().is_empty() {
                    words.push(text.trim().to_string());
                }
            }
            words
        }
        Some(_) => return Err(format!("section {n} of the answer had {key} that were not a list")),
    };
    if had && words.is_empty() {
        return Err(format!("section {n} of the answer had no {key} where the report had some"));
    }
    Ok(words)
}

/// **ONE SECTION OF RICH'S ANSWER, OR NO ANSWER**: section `n` must be an object with a heading in
/// words, and its paragraphs and steps as [`words_of`] reads them, against `ours`, the scanner's.
/// Nothing is filled in for him: a section that is not all there is a malformed answer, refused
/// like a Claude failure (not offered, kept, asked again).
fn section_of(his: &serde_json::Value, n: usize, ours: &DraftSection) -> Result<(String, Vec<String>, Vec<String>), String> {
    let section = his.as_object().ok_or_else(|| format!("section {n} of the answer was not a section"))?;
    let heading = section.get("heading").and_then(|h| h.as_str()).map(str::trim).unwrap_or("");
    if heading.is_empty() {
        return Err(format!("section {n} of the answer had no heading"));
    }
    let paragraphs = words_of(section, "paragraphs", n, !ours.paragraphs.is_empty())?;
    let steps = words_of(section, "steps", n, !ours.steps.is_empty())?;
    Ok((heading.to_string(), paragraphs, steps))
}

/// **RICH'S LAST WORDS AS THE CARD'S DRAFT, OR NONE**: `rich` is his answer to [`finish_prompt`]
/// about `scanned`. His title, headings, paragraphs and steps are the draft's words exactly, his
/// sections in order. An answer with no title, another number of sections, a section that is not
/// all there ([`section_of`]: not an object, no heading, a list that is not words, or text the
/// report had gone), or a malformed `private` list ([`private_of`]) is no answer, and the report
/// waits.
///
/// Nothing here changes a word of his. Each stand-in in his words is only MARKED for the card's
/// tooltip ([`marked`]): with what the stand-in in the same place of the report replaced, the
/// scanner's or what he says he replaced, and with its kind alone where that place cannot be
/// told (review rv-20261010T072702Z-9463220e-543f finding 1). **EVERY stand-in in his words is marked**, whatever he listed beside them: one nobody
/// accounts for is marked with its kind and no replaced words ([`Segment::unaccounted`]), so the
/// card underlines it and the line under it counts it (nightly 48's walk, D23: on d0d06501c it
/// stayed plain words, and "Nothing private was in it" sat under four stand-ins Rich had written
/// himself, his `private` list empty because nothing private was still in his words). What the scanner hid and he put
/// back is in his words as it was, so it marks no stand-in. What he replaced is kept with the
/// report's private words, so the scanner leaves it out of every later change too.
pub fn finished(rich: Result<String, String>, scanned: &Draft) -> Result<Draft, String> {
    let value = json_object(&rich?)?;
    let title = text_of(&value, "title");
    let theirs = value["sections"].as_array().ok_or("the answer had no sections")?;
    if title.is_empty() || theirs.len() != scanned.sections.len() {
        return Err(format!("the answer had no title, or {} sections for the report's {}", theirs.len(), scanned.sections.len()));
    }
    let his_sections =
        scanned.sections.iter().zip(theirs).enumerate().map(|(n, (ours, his))| section_of(his, n + 1, ours)).collect::<Result<Vec<_>, String>>()?;
    let replaced = private_of(&value)?;
    // **EVERY STAND-IN OF THE REPORT AS THE SCANNER LEFT IT, IN ITS PLACE** (review
    // rv-20261010T072702Z-9463220e-543f finding 1): the report as [`Piece`]s, its texts (title,
    // then each section's paragraphs and steps) one after another. A stand-in the scanner wrote
    // carries what it replaced; one already in the words carries `None`, which nothing here knows
    // (one Rich wrote before, kept through Change it).
    let mut report: Vec<Piece> = Vec::new();
    let units = std::iter::once(&scanned.title).chain(scanned.sections.iter().flat_map(|s| s.paragraphs.iter().chain(&s.steps)));
    for (n, unit) in units.enumerate() {
        if n > 0 {
            push_words(&mut report, "\n");
        }
        for s in unit {
            match s.kind {
                Some(kind) => report.push(Piece::StandIn(kind, s.was.clone())),
                None => {
                    let mut at = 0;
                    for (start, kind) in stand_ins_in(&s.text) {
                        push_words(&mut report, &s.text[at..start]);
                        report.push(Piece::StandIn(kind, None));
                        at = start + kind.stand_in().len();
                    }
                    push_words(&mut report, &s.text[at..]);
                }
            }
        }
    }
    // **A STAND-IN NOT TIED TO ITS PLACE CLAIMS NOTHING** (review
    // rv-20261010T081721Z-66ba5386-a4d1 finding 1): what each stand-in in his words replaced, read
    // off the one way his words are the report's ([`receipts`]), or only its kind where that way
    // is not certain. On 66ba53861 what he replaced took the first place its words were found,
    // whole or not, so in "Mary Ann saw it. Bob saw it. Ann saw it too." Ann was found inside
    // Mary Ann, and Bob's and Ann's receipts swapped; and what he put back was counted by its
    // words, not its place, so the wrong one of two hidden names could be taken as put back.
    let his_texts: Vec<&str> =
        std::iter::once(title.as_str()).chain(his_sections.iter().flat_map(|(_, paragraphs, steps)| paragraphs.iter().chain(steps)).map(String::as_str)).collect();
    let mut queue = receipts(&report, &his_texts, &replaced);
    let mut mark = |text: &str| marked(text, &mut queue);
    let title = mark(&title);
    let sections = his_sections
        .into_iter()
        .map(|(heading, paragraphs, steps)| DraftSection {
            heading,
            paragraphs: paragraphs.iter().map(|p| mark(p)).collect(),
            steps: steps.iter().map(|s| mark(s)).collect(),
        })
        .collect();
    Ok(Draft { title, sections, private: distinct(scanned.private.iter().cloned().chain(replaced)) })
}

/// Each stand-in written in `text` ([`Kind::stand_in`]), where it starts and its kind, in order.
fn stand_ins_in(text: &str) -> Vec<(usize, Kind)> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some((start, kind)) =
        Kind::ALL.iter().filter_map(|k| text[at..].find(k.stand_in()).map(|i| (at + i, *k))).min_by_key(|&(i, k)| (i, std::cmp::Reverse(k.stand_in().len())))
    {
        found.push((start, kind));
        at = start + kind.stand_in().len();
    }
    found
}

/// `text` as segments, its words unchanged: each stand-in in it ([`stand_ins_in`]) marked with the
/// first entry of `queue` of its kind, which it then takes; one whose entry knows no words, or
/// with no entry, is marked with its kind alone ([`Segment::unaccounted`]). Every stand-in written
/// is a stand-in on the card.
fn marked(text: &str, queue: &mut Vec<(Kind, Option<String>)>) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let plain = |out: &mut Vec<Segment>, words: &str| match out.last_mut() {
        Some(last) if !last.is_stand_in() => last.text.push_str(words),
        _ if !words.is_empty() => out.push(Segment::plain(words)),
        _ => {}
    };
    let mut at = 0;
    for (start, kind) in stand_ins_in(text) {
        plain(&mut out, &text[at..start]);
        match queue.iter().position(|(k, _)| *k == kind).map(|i| queue.remove(i).1) {
            Some(Some(was)) => out.push(Segment::stand_in(&was, kind)),
            _ => out.push(Segment::unaccounted(kind)),
        }
        at = start + kind.stand_in().len();
    }
    plain(&mut out, &text[at..]);
    if out.is_empty() {
        out.push(Segment::plain(""));
    }
    out
}

/// One piece of the report as the scanner left it ([`finished`]): words, or a stand-in with
/// what it replaced (`None` for one already in the words, which nothing here knows).
enum Piece {
    Words(String),
    StandIn(Kind, Option<String>),
}

/// `text` added to the words at the end of `pieces`, or as new words after a stand-in.
fn push_words(pieces: &mut Vec<Piece>, text: &str) {
    match pieces.last_mut() {
        Some(Piece::Words(words)) => words.push_str(text),
        _ if !text.is_empty() => pieces.push(Piece::Words(text.to_string())),
        _ => {}
    }
}

/// `text` with every run of white space written as one space: a paragraph split or joined
/// differently, or a line break written as a space, is still the same words.
fn one_spaced(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut gap = false;
    for c in text.chars() {
        if c.is_whitespace() {
            if !gap {
                out.push(' ');
            }
            gap = true;
        } else {
            out.push(c);
            gap = false;
        }
    }
    out
}

/// The most ways of reading a report [`receipts`] follows before it calls the report uncertain:
/// far above any real report (one way per character per place it could be), and a bound on what a
/// pathological one costs.
const MOST_READINGS: usize = 400_000;

/// **WHAT EACH STAND-IN IN RICH'S WORDS REPLACED, ONLY WHERE THAT IS CERTAIN** (review
/// rv-20261010T081721Z-66ba5386-a4d1 finding 1). `his` are his texts in the order of `report`'s
/// (title, then each section's paragraphs and steps); the answer has one entry per stand-in in
/// them ([`stand_ins_in`]), in order, for [`marked`].
///
/// His words are the report's with only two kinds of change: a stand-in the scanner wrote put
/// back as the words it replaced, and words of `replaced` (what he says he replaced now) written
/// as the stand-in for their kind. Every reading of the report under those changes that gives
/// exactly his words, white space aside ([`one_spaced`]), is followed, character by character:
/// each reading ties each of his stand-ins to one place, a whole stand-in of the report or the
/// whole words of a replaced term, never part of either and never two at once. A stand-in names
/// what it replaced only when EVERY reading names the same; and when any stand-in of a kind is
/// in doubt (two readings disagree), or no reading gives his words at all (a word changed, a
/// stand-in nothing accounts for, a count that does not reconcile), every stand-in of that kind,
/// or of every kind, says only its kind ([`Segment::unaccounted`]).
fn receipts(report: &[Piece], his: &[&str], replaced: &[PrivateTerm]) -> Vec<(Kind, Option<String>)> {
    let in_order: Vec<Kind> = his.iter().flat_map(|text| stand_ins_in(text)).map(|(_, kind)| kind).collect();
    let in_doubt = || in_order.iter().map(|&kind| (kind, None)).collect::<Vec<_>>();
    let words = one_spaced(&his.join("\n"));
    let words = words.trim();
    let spans = stand_ins_in(words);
    // Spacing must leave his stand-ins as they were: one written across a line break is a stand-in
    // to the reading and not to the card.
    if spans.iter().map(|&(_, kind)| kind).ne(in_order.iter().copied()) {
        return in_doubt();
    }
    let span_at: std::collections::HashMap<usize, (usize, Kind)> = spans.iter().enumerate().map(|(n, &(at, kind))| (at, (n, kind))).collect();
    let mut inside = vec![false; words.len() + 1];
    for &(at, kind) in &spans {
        inside[at..at + kind.stand_in().len()].iter_mut().for_each(|b| *b = true);
    }
    let clear = |from: usize, to: usize| !inside[from..to].iter().any(|&b| b);
    // The report's words spaced the same way, the ends of the whole trimmed as his are.
    let mut pieces: Vec<Piece> = report
        .iter()
        .map(|p| match p {
            Piece::Words(w) => Piece::Words(one_spaced(w)),
            Piece::StandIn(kind, was) => Piece::StandIn(*kind, was.clone()),
        })
        .collect();
    let last = pieces.len().saturating_sub(1);
    if let Some(Piece::Words(w)) = pieces.first_mut() {
        *w = w.trim_start().to_string();
    }
    if let Some(Piece::Words(w)) = pieces.get_mut(last) {
        *w = w.trim_end().to_string();
    }
    let terms: Vec<(String, &PrivateTerm)> = replaced.iter().map(|t| (one_spaced(&t.text), t)).filter(|(spaced, _)| !spaced.trim().is_empty()).collect();

    // A reading's place: the piece, how far into its words, and how far into his.
    type Place = (usize, usize, usize);
    let mut number: std::collections::HashMap<Place, usize> = std::collections::HashMap::new();
    let mut places: Vec<Place> = vec![(0, 0, 0)];
    number.insert((0, 0, 0), 0);
    // The stand-in of his a step ties, if any: its number among his, and what it replaced.
    type Tie = Option<(usize, Option<String>)>;
    // Each step: from, to, and what it ties.
    let mut steps: Vec<(usize, usize, Tie)> = Vec::new();
    let mut todo = vec![0];
    while let Some(from) = todo.pop() {
        let (i, j, p) = places[from];
        let mut next: Vec<(Place, Tie)> = Vec::new();
        match pieces.get(i) {
            None => {}
            Some(Piece::Words(w)) if j == w.len() => next.push(((i + 1, 0, p), None)),
            Some(Piece::Words(w)) => {
                // The same character, outside his stand-ins.
                if let Some(c) = w[j..].chars().next() {
                    let n = c.len_utf8();
                    if words[p..].starts_with(c) && clear(p, p + n) {
                        next.push(((i, j + n, p + n), None));
                    }
                }
                // Or the whole words of a term he replaced, as the whole stand-in of its kind.
                if let Some(&(span, kind)) = span_at.get(&p) {
                    for (spaced, term) in &terms {
                        if term.kind == kind && w[j..].starts_with(spaced.as_str()) {
                            next.push(((i, j + spaced.len(), p + kind.stand_in().len()), Some((span, Some(term.text.clone())))));
                        }
                    }
                }
            }
            Some(Piece::StandIn(kind, was)) => {
                // Kept: the whole stand-in, in the same place.
                if let Some(&(span, k)) = span_at.get(&p) {
                    if k == *kind {
                        next.push(((i + 1, 0, p + k.stand_in().len()), Some((span, was.clone()))));
                    }
                }
                // Or put back: the whole words it replaced, outside his stand-ins.
                if let Some(back) = was.as_deref().map(one_spaced) {
                    if !back.trim().is_empty() && words[p..].starts_with(back.as_str()) && clear(p, p + back.len()) {
                        next.push(((i + 1, 0, p + back.len()), None));
                    }
                }
            }
        }
        for (place, tie) in next {
            let to = match number.get(&place) {
                Some(&to) => to,
                None => {
                    places.push(place);
                    number.insert(place, places.len() - 1);
                    todo.push(places.len() - 1);
                    places.len() - 1
                }
            };
            steps.push((from, to, tie));
        }
        if places.len() > MOST_READINGS {
            return in_doubt();
        }
    }
    // The places a reading that gives all of his words passes through, walked back from its end.
    let mut whole = vec![false; places.len()];
    let mut back: Vec<Vec<usize>> = vec![Vec::new(); places.len()];
    for &(from, to, _) in &steps {
        back[to].push(from);
    }
    let mut walk: Vec<usize> = (0..places.len()).filter(|&n| places[n].0 == pieces.len() && places[n].2 == words.len()).collect();
    for &n in &walk {
        whole[n] = true;
    }
    while let Some(n) = walk.pop() {
        for &from in &back[n] {
            if !whole[from] {
                whole[from] = true;
                walk.push(from);
            }
        }
    }
    if !whole[0] {
        return in_doubt();
    }
    // What each of his stand-ins replaced, in every whole reading.
    let mut named: Vec<Vec<Option<String>>> = vec![Vec::new(); spans.len()];
    for (_, to, tie) in steps {
        if let (true, Some((span, was))) = (whole[to], tie) {
            if !named[span].contains(&was) {
                named[span].push(was);
            }
        }
    }
    let doubted: std::collections::HashSet<Kind> = spans.iter().zip(&named).filter(|(_, names)| names.len() != 1).map(|(&(_, kind), _)| kind).collect();
    spans.iter().zip(named).map(|(&(_, kind), mut names)| (kind, if doubted.contains(&kind) { None } else { names.pop().flatten() })).collect()
}

/// **A REPORT RICH COULD NOT CHECK YET**, kept on this Mac until he can: the user's words and
/// where they were, never a draft, so nothing of it can be sent. [`Outbox::check_due`] asks him
/// again; his check waits here (`checked`) until the window has shown it and taken it
/// ([`Outbox::take_unchecked`]), so a window that was closed or reloaded meanwhile loses nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unchecked {
    pub id: String,
    pub created_at_ms: u64,
    /// What the user said, in their own words. Private; never part of an issue.
    pub answer: String,
    /// Where they were, the screen's words included. Private; never part of an issue.
    pub screen: Screen,
    /// How many times Rich has been asked.
    pub attempts: u32,
    pub next_try_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<Checked>,
    /// Words the user changed by hand that Rich could not check at Send ([`Outbox::keep_edited`]):
    /// he is asked about these ([`checked_edit`]) rather than to write `answer` up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited: Option<Edited>,
}

/// How long a report Rich could not check rests before he is asked again: one `claude --print`
/// turn every two minutes at most, while one waits.
pub const CHECK_RETRY_MS: u64 = 120_000;

/// What writing a report came to: Rich's checked draft, or the report kept on this Mac until he
/// can check it (`id`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum WriteUp {
    Checked(Checked),
    Unchecked { id: String },
}

/// A change said to Rich: the heading it goes under, the words to add, and any private words
/// Rich names in them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub section: String,
    pub add: String,
    pub private: Vec<PrivateTerm>,
}

pub fn parse_change(raw: &str) -> Result<Change, String> {
    let value = json_object(raw)?;
    let change = Change { section: text_of(&value, "section"), add: text_of(&value, "add"), private: private_of(&value)? };
    if change.section.is_empty() || change.add.is_empty() {
        return Err("the answer had no section or nothing to add".into());
    }
    Ok(change)
}

/// **A CHANGE, SCRUBBED WITH EVERYTHING THE REPORT KNOWS IS PRIVATE**: the names RichOS holds
/// (`app`), the ones Rich found while writing the report and every change before this one
/// (`report`), and any he names in this change. Returns the words to add and the report's
/// private words from now on, so a name Rich found once stays left out on every later change
/// (second review finding 4: on 1c3dda1dc his names applied to the first draft only).
///
/// **EVERY STAND-IN IN HIS ADDITION IS MARKED AND COUNTED, AS IN THE FIRST DRAFT** (review
/// rv-20261010T180313Z-f29db708-733e finding 1): his prompt asks him to write private details as
/// stand-ins, and on f29db7082 the ones he wrote stayed plain words, so the card said "Nothing
/// private was in it" under "[a person] could not open [a file on this Mac] either." Each is marked
/// by [`marked`], the first draft's rule; his change lists what is still in his words, never what
/// he replaced, so nothing accounts for one and each says its kind alone ([`Segment::unaccounted`]).
pub fn scrub_change(change: &Change, app: &[PrivateTerm], report: &[PrivateTerm]) -> (Vec<Segment>, Vec<PrivateTerm>) {
    let report = distinct(report.iter().chain(&change.private).cloned());
    let scrubber = Scrubber::with_rich(app.to_vec(), report.clone());
    let add = scrubber
        .scrub(&change.add)
        .into_iter()
        .flat_map(|segment| if segment.is_stand_in() { vec![segment] } else { marked(&segment.text, &mut Vec::new()) })
        .collect();
    (add, report)
}

/// **THE CARD'S HEADS-UP, ASKED OF THE SCRUBBER THAT CLEANS THE REPORT**: the private words in
/// `text`, the words of the report as the user has changed them, by exactly the rules and the
/// private words a change is scrubbed with ([`scrub_change`]): the names RichOS holds (`app`) and
/// the ones Rich found in this report (`report`). The window keeps no copy of these rules (fourth
/// review finding 2: its own ASCII-only email pattern missed `alice@büro.de`, which this catches).
/// It decides nothing: the user may send anyway, having been told.
pub fn private_in_edit(text: &str, app: &[PrivateTerm], report: &[PrivateTerm]) -> Vec<String> {
    Scrubber::with_rich(app.to_vec(), report.to_vec()).private_in(text)
}

/// One section of the draft card, with stand-ins in place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftSection {
    pub heading: String,
    pub paragraphs: Vec<Vec<Segment>>,
    pub steps: Vec<Vec<Segment>>,
}

/// The report as the card first shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub title: Vec<Segment>,
    pub sections: Vec<DraftSection>,
    /// The private words Rich found in this report beyond the ones RichOS holds. The card keeps
    /// them for every later change ([`scrub_change`]) and for its heads-up ("Jane Doe" looks private).
    #[serde(default)]
    pub private: Vec<PrivateTerm>,
}

/// **A SECTION'S PARAGRAPHS, SCRUBBED WHOLE, THEN SPLIT**: `text` is scrubbed as one string, and
/// only then split at its blank lines into paragraphs, so a private name that runs across a blank
/// line is one stand-in in one paragraph (review rv-20261009T174727Z-3007e320-578a finding 2: on
/// 3007e3200 the split came first, and "Jane\n\nDoe" became the paragraphs "Jane" and "Doe saw
/// the window freeze.", neither of them the name). A stand-in is never split; a single line break
/// inside a paragraph reads as a space, as before. Empty paragraphs are dropped.
fn paragraphs_of(text: &str, scrubber: &Scrubber) -> Vec<Vec<Segment>> {
    let mut out: Vec<Vec<Segment>> = vec![vec![]];
    for segment in scrubber.scrub(text) {
        if segment.is_stand_in() {
            out.last_mut().expect("never empty").push(segment);
            continue;
        }
        for (n, piece) in segment.text.split("\n\n").enumerate() {
            if n > 0 {
                out.push(vec![]);
            }
            out.last_mut().expect("never empty").push(Segment::plain(&piece.replace('\n', " ")));
        }
    }
    out.into_iter()
        .filter_map(|mut paragraph| {
            if let Some(first) = paragraph.first_mut().filter(|s| !s.is_stand_in()) {
                first.text = first.text.trim_start().to_string();
            }
            if let Some(last) = paragraph.last_mut().filter(|s| !s.is_stand_in()) {
                last.text = last.text.trim_end().to_string();
            }
            paragraph.retain(|s| s.is_stand_in() || !s.text.is_empty());
            (!paragraph.is_empty()).then_some(paragraph)
        })
        .collect()
}

/// Rich's write-up through the scanner: every section scrubbed, the version line added as it is, and the
/// whole of it scanned as GitHub would show it ([`as_posted`]). Rich gets the last word on it ([`checked`]).
pub fn draft_from(written: &Written, version: &str, scrubber: &Scrubber) -> Draft {
    let scrubber = scrubber.and_rich(written.private.iter().cloned());
    let mut sections: Vec<DraftSection> = written
        .laid_out()
        .into_iter()
        .map(|(heading, text, steps)| DraftSection {
            heading: heading.into(),
            paragraphs: paragraphs_of(text, &scrubber),
            steps: steps.iter().map(|s| scrubber.scrub(s)).collect(),
        })
        .collect();
    sections.push(DraftSection { heading: "Version".into(), paragraphs: vec![vec![Segment::plain(version)]], steps: vec![] });
    as_posted(&Draft { title: scrubber.scrub(&written.title), sections, private: distinct(written.private.iter().cloned()) }, &scrubber)
}

// ---------------------------------------------------------------------------------------
// THE ISSUE, WORD FOR WORD
// ---------------------------------------------------------------------------------------

/// The sheet as the user approved it: plain words, stand-ins included as their words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sheet {
    pub title: String,
    pub sections: Vec<SheetSection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SheetSection {
    pub heading: String,
    #[serde(default)]
    pub paragraphs: Vec<String>,
    #[serde(default)]
    pub steps: Vec<String>,
}

/// **EVERY MARK THAT COULD MAKE GITHUB SHOW OTHER WORDS THAN THE ONES WRITTEN**, escaped in an
/// issue's body ([`escaped`]) so that what GitHub shows is the words themselves: emphasis, code,
/// links, HTML, headings, tables, and since review rv-20261009T174727Z-3007e320-578a also `~`
/// (strikethrough: "~~Jane~~ Doe" shows "Jane Doe") and `&` (an entity: "Jane&#32;Doe" shows
/// "Jane Doe"). The last scrub reads the body as shown ([`shown`]), so a mark left unescaped
/// here would be a way for a name to be shown that the scrub did not read.
const MARKS: &str = "\\`*_[]<>#|~&";

/// **THE MARKS THAT ARE MARKDOWN ONLY AT THE START OF A LINE** (review
/// rv-20261010T194410Z-796ff877-381f: "- The button does nothing." went out as a bullet, "---" as a
/// divider): a list's `-` or `+`, a line of `=` under a line (a heading), and the `.` or `)` after
/// a line's leading number (a numbered list). [`escaped_lines`] escapes them there only; `*`, `_`,
/// `#` and `>` are in [`MARKS`] and escaped everywhere already. [`shown`] reads each as itself.
const LINE_MARKS: &str = "-+=.)";

/// **WHAT FOLLOWS EVERY `@` IN AN ISSUE'S BODY**: a zero-width space, so "@octocat" is not a
/// mention: GitHub neither links it nor notifies that account, and the reader still sees
/// "@octocat" (review rv-20261009T183159Z-a2fc94dc-20e7, the leak cc/echo-opus-bug13's handover
/// named). It is the character Renovate has put after `@` in the pull requests it writes, for the
/// same reason.
const MENTION_BREAK: char = '\u{200B}';

/// Markdown's punctuation, escaped so GitHub shows the words the user read rather than
/// formatting they never saw ([`MARKS`]), and every `@` followed by [`MENTION_BREAK`] so no
/// one on GitHub is mentioned. Every word of the body, and every stand-in the last scrub puts
/// in it ([`Scrubber::scrub_markdown`]), is written by this, so every `@` that is sent has it.
fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if MARKS.contains(c) {
            out.push('\\');
        }
        out.push(c);
        if c == '@' {
            out.push(MENTION_BREAK);
        }
    }
    out
}

/// The card's words as [`escaped`] writes them, each line without the spaces and tabs it starts
/// with, and on every line the mark that would start a Markdown block there ([`LINE_MARKS`])
/// escaped too: a leading `-`, `+` or `=`, and the `.` or `)` after a leading number. So the line
/// GitHub shows is the line the user approved, not a list, a divider or a heading. Indentation
/// means nothing in a bug report and is dropped (review rv-20261010T204141Z-53407e2a-dc94: a line
/// indented four spaces was a code block, where the escapes show), so no line is a code block and
/// every escape is on the line's real first character.
///
/// Public for RichDoctor (richos-hq/docs/prds/2026-10-22-richdoctor-v1.md (the private record) §9: reuse the
/// tested primitives rather than copy them), whose reports must escape exactly as Bust a bug's
/// do: the shared reporting relay rejects any `<`, `[` or `]` this did not escape.
pub fn escaped_lines(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            let rest = line.trim_start_matches([' ', '\t']);
            let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            let (number, rest) = rest.split_at(digits);
            match rest.chars().next() {
                Some(c @ ('.' | ')')) if !number.is_empty() => format!("{number}\\{c}{}", escaped(&rest[1..])),
                Some(c @ ('-' | '+' | '=')) if number.is_empty() => format!("\\{c}{}", escaped(&rest[1..])),
                _ => escaped(&format!("{number}{rest}")),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// **THE WORDS GITHUB SHOWS** for a body this module wrote ([`issue_body`]): every escaped mark
/// read as the mark itself, and the [`MENTION_BREAK`] after each `@` read as nothing, so these
/// are the words as the user wrote them (an email address is whole again for the scrub). With
/// them, where each of their characters starts in `markdown`, as (offset in the words shown,
/// offset in `markdown`), in order.
fn shown(markdown: &str) -> (String, Vec<(usize, usize)>) {
    let mut words = String::with_capacity(markdown.len());
    let mut at = Vec::with_capacity(markdown.len());
    let mut chars = markdown.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let c = match chars.peek() {
            Some(&(_, next)) if c == '\\' && (MARKS.contains(next) || LINE_MARKS.contains(next)) => {
                chars.next();
                next
            }
            Some(&(_, MENTION_BREAK)) if c == '@' => {
                chars.next();
                c
            }
            _ => c,
        };
        at.push((words.len(), i));
        words.push(c);
    }
    (words, at)
}

/// Where some of the card's words are on it: a section's heading, one of its paragraphs, or one
/// of its steps (by section, then by paragraph or step).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    Heading(usize),
    Paragraph(usize, usize),
    Step(usize, usize),
}

/// One part of an issue's body: some of the card's words with where they are on it, or a mark
/// written around them (`None`).
type Part = (Option<Place>, String);

/// **AN ISSUE'S BODY, PART BY PART, IN ORDER**: the card's own words with where they are on it,
/// and the marks this module writes around them (`None`: "### ", a step's number, line breaks).
/// The body ([`issue_body`]) and the words the last scrub reads in it ([`as_posted`]) are both
/// written from these, so the two cannot be laid out differently.
fn body_parts(sheet: &Sheet) -> Vec<Part> {
    let mut parts: Vec<Part> = Vec::new();
    let mark = |parts: &mut Vec<Part>, text: &str| parts.push((None, text.to_string()));
    for (i, section) in sheet.sections.iter().enumerate() {
        if i > 0 {
            mark(&mut parts, "\n");
        }
        mark(&mut parts, "### ");
        parts.push((Some(Place::Heading(i)), section.heading.trim().to_string()));
        mark(&mut parts, "\n");
        for (j, p) in section.paragraphs.iter().enumerate().map(|(j, p)| (j, p.trim())).filter(|(_, p)| !p.is_empty()) {
            mark(&mut parts, "\n");
            parts.push((Some(Place::Paragraph(i, j)), p.to_string()));
            mark(&mut parts, "\n");
        }
        let steps: Vec<(usize, &str)> = section.steps.iter().enumerate().map(|(k, s)| (k, s.trim())).filter(|(_, s)| !s.is_empty()).collect();
        if !steps.is_empty() {
            mark(&mut parts, "\n");
            // An idea's pictured points are bullets, a bug report's steps are numbered ([`BULLETED`]).
            let bulleted = section.heading.trim() == BULLETED;
            for (n, (k, step)) in steps.into_iter().enumerate() {
                mark(&mut parts, &if bulleted { "- ".to_string() } else { format!("{}. ", n + 1) });
                parts.push((Some(Place::Step(i, k)), step.to_string()));
                mark(&mut parts, "\n");
            }
        }
    }
    parts
}

/// The issue's body: each section as a heading and its words, nothing added, every word escaped
/// ([`escaped_lines`]) so GitHub shows the words themselves.
pub fn issue_body(sheet: &Sheet) -> String {
    body_parts(sheet).into_iter().map(|(place, text)| if place.is_some() { escaped_lines(&text) } else { text }).collect()
}

/// The page of issue `number` on [`REPOSITORY`]. Built from a number, so the card's link can only
/// ever open an issue of this repository, whatever the webview asks for.
pub fn issue_page(number: u64) -> String {
    format!("https://github.com/{REPOSITORY}/issues/{number}")
}

/// The page of discussion `number` on [`REPOSITORY`] (a posted idea). Built from a number, as
/// [`issue_page`] is, so the card's link can only ever open a discussion of this repository.
pub fn discussion_page(number: u64) -> String {
    format!("https://github.com/{REPOSITORY}/discussions/{number}")
}

/// **THE PAGE A SENT CARD OPENS**: `url`, the link the relay answered with, when it is exactly a
/// page of `post`'s kind (an issue, or an idea's discussion) on the configured repository
/// (`configured`; RichOS's repository under either of its addresses, [`crate::github_home::accepted`]).
/// Anything else the webview asks for is refused, so the card can only ever open a post of this
/// repository.
pub fn page_to_open(url: &str, post: Post, configured: &str) -> Option<String> {
    let kind = match post {
        Post::Bug => "issues",
        Post::Idea => "discussions",
    };
    crate::github_home::page_number(url, &crate::github_home::accepted(configured), kind).map(|_| url.to_string())
}

/// The API path and JSON body that file `public` on [`REPOSITORY`]: its title and its body, and
/// nothing else (no labels, no assignees).
pub fn issue_request(public: &Public) -> (String, serde_json::Value) {
    (format!("/repos/{REPOSITORY}/issues"), serde_json::json!({ "title": public.title(), "body": public.body() }))
}

// ---------------------------------------------------------------------------------------
// WHAT LEAVES THIS MAC: ONE DECISION, AT THE ONE PLACE A REPORT LEAVES
// ---------------------------------------------------------------------------------------
//
// The order is the CEO's (§115, 2026-10-09): THE SCANNER RUNS FIRST, RICH RUNS LAST, and nothing
// after Rich changes the words but the escaping GitHub needs. One place decides ([`at_send`]), and
// nothing else sends ([`Outbox::send`] takes only a [`Public`], and only `at_send` makes one):
//
//   (a) the words sent are the card's words when Send was pressed, every one, written out with
//       nothing changed ([`issue_of`]);
//   (b) they are words Rich gave last, to the character ([`Check`]): his cleaned-up draft as the
//       card first showed it. Words that differ (the user changed them by hand, or told Rich a
//       change) are put through the scanner ([`redraft`], which runs [`as_posted`]: the title's one
//       substitution and the scan of each field as one whole string) and then given to Rich,
//       beside the words as they were typed ([`as_typed`]), to put back what is not private and
//       clean up what is ([`finish_prompt`]); nothing is sent until he has, and his version is shown on the
//       card unless it is the card's words exactly. When Claude cannot answer, the words wait on
//       this Mac ([`Outbox::keep_edited`]).

/// **THE WORDS OF ONE ISSUE, WRITTEN OUT FROM THE CARD**: its title as the card shows it, and its
/// body ([`issue_body`]): the card's words, every one as it reads, with only the marks that make
/// GitHub show them as written ([`escaped`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub title: String,
    pub body: String,
}

/// The issue `sheet` is written out as. It changes no word.
pub fn issue_of(sheet: &Sheet) -> Issue {
    Issue { title: sheet.title.trim().to_string(), body: issue_body(sheet) }
}

/// **THE SCANNER'S PASS OVER A WHOLE REPORT, AS GITHUB WOULD SHOW IT** ([`draft_from`],
/// [`redraft`]), before Rich's last pass ([`finished`]):
///
///   - the title's one substitution: a backtick is written as an apostrophe, because GitHub draws a
///     title's `word` as code, and a title shows the words as they are (the scrub reads them so);
///   - the last scrub, with `scrubber`: the title as one whole string, and the whole body as one
///     whole string, its headings, paragraphs and steps with the marks around them, read as GitHub
///     shows them. What it leaves out is left out where it is on the card: a heading, a paragraph,
///     a step (its number stays, it is not private), and a name that runs from one into the next
///     is left out in each;
///   - again, until nothing more is left out: a stand-in is never private ([`outside_stand_ins`]),
///     so every pass leaves out words that were there before it, and the passes end.
///
/// Stand-ins already on `draft` stay as they are, with what they replaced.
pub fn as_posted(draft: &Draft, scrubber: &Scrubber) -> Draft {
    let mut card = tidied(draft);
    for segment in card.title.iter_mut().filter(|s| !s.is_stand_in()) {
        segment.text = segment.text.replace('`', "'");
    }
    loop {
        let sheet = sheet_of(&card);
        let title = scrubber.spans(&sheet.title);
        let mut in_body: Vec<(Place, Vec<Span>)> = Vec::new();
        let parts = body_parts(&sheet);
        let words: String = parts.iter().map(|(_, text)| text.as_str()).collect();
        let spans = scrubber.spans(&words);
        let mut at = 0;
        for (place, text) in &parts {
            let (start, end) = (at, at + text.len());
            at = end;
            let Some(place) = place else { continue };
            let here: Vec<Span> =
                spans.iter().filter(|&&(s, e, _)| s < end && e > start).map(|&(s, e, kind)| (s.max(start) - start, e.min(end) - start, kind)).collect();
            let here = outside_stand_ins(text, here);
            if !here.is_empty() {
                in_body.push((*place, here));
            }
        }
        if title.is_empty() && in_body.is_empty() {
            return card;
        }
        card.title = left_out(&card.title, &title);
        for (place, here) in in_body {
            match place {
                Place::Heading(i) => {
                    let heading = &mut card.sections[i].heading;
                    *heading = left_out(&[Segment::plain(heading)], &here).iter().map(|s| s.text.as_str()).collect();
                }
                Place::Paragraph(i, j) => card.sections[i].paragraphs[j] = left_out(&card.sections[i].paragraphs[j], &here),
                Place::Step(i, k) => card.sections[i].steps[k] = left_out(&card.sections[i].steps[k], &here),
            }
        }
        card = tidied(&card);
        // Every pass leaves out words that were not a stand-in; one that leaves nothing out ends.
        if sheet_of(&card) == sheet {
            return card;
        }
    }
}

/// `segments` with each of `spans` (offsets in their words, in order, not overlapping) left out:
/// a stand-in in its place, with what it replaced. A stand-in already there stays as it is.
fn left_out(segments: &[Segment], spans: &[Span]) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut at = 0;
    for segment in segments {
        let end = at + segment.text.len();
        if segment.is_stand_in() {
            out.push(segment.clone());
            at = end;
            continue;
        }
        let mut done = at;
        for &(s, e, kind) in spans.iter().filter(|&&(s, e, _)| s < end && e > at) {
            let (s, e) = (s.max(done), e.min(end));
            if s >= e {
                continue;
            }
            if s > done {
                out.push(Segment::plain(&segment.text[done - at..s - at]));
            }
            out.push(Segment::stand_in(&segment.text[s - at..e - at], kind));
            done = e;
        }
        if done < end || segment.text.is_empty() {
            out.push(Segment::plain(&segment.text[done - at..]));
        }
        at = end;
    }
    out
}

/// **THE CARD AS THE WINDOW READS IT** ([`sheet_of`]), one segment list per paragraph and step it
/// reads: plain words next to each other joined, the spaces at either end of each trimmed,
/// paragraphs and steps with no words dropped, headings trimmed. A "stand-in" whose words are not
/// a stand-in is plain words: only a stand-in is never scrubbed again ([`outside_stand_ins`]).
fn tidied(draft: &Draft) -> Draft {
    let words = |segments: &[Segment]| -> Vec<Segment> {
        let mut out: Vec<Segment> = Vec::new();
        for segment in segments {
            let stand_in = segment.is_stand_in() && Kind::ALL.iter().any(|k| k.stand_in() == segment.text);
            match out.last_mut() {
                Some(last) if !stand_in && !last.is_stand_in() => last.text.push_str(&segment.text),
                _ if stand_in => out.push(segment.clone()),
                _ => out.push(Segment::plain(&segment.text)),
            }
        }
        if let Some(first) = out.first_mut().filter(|s| !s.is_stand_in()) {
            first.text = first.text.trim_start().to_string();
        }
        if let Some(last) = out.last_mut().filter(|s| !s.is_stand_in()) {
            last.text = last.text.trim_end().to_string();
        }
        out.retain(|s| s.is_stand_in() || !s.text.is_empty());
        out
    };
    let kept = |list: &[Vec<Segment>]| list.iter().map(|p| words(p)).filter(|p| !p.is_empty()).collect();
    Draft {
        title: words(&draft.title),
        sections: draft
            .sections
            .iter()
            .map(|s| DraftSection { heading: s.heading.trim().to_string(), paragraphs: kept(&s.paragraphs), steps: kept(&s.steps) })
            .collect(),
        private: draft.private.clone(),
    }
}

/// **WORDS THAT MAY LEAVE THIS MAC**: the card's words at Send, which are words Rich gave last.
/// Made only by [`at_send`]; [`Outbox::send`] and [`issue_request`] take nothing else, so there is
/// no other way for a report to reach GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Public(Issue);

impl Public {
    pub fn title(&self) -> &str {
        &self.0.title
    }
    pub fn body(&self) -> &str {
        &self.0.body
    }
}

/// **WHAT RICH LAST GAVE OF ONE REPORT**: its words exactly as the card showed them then
/// ([`issue_of`]), and the report's private words so far (his and the scanner's, for the next
/// scan). Made only from a draft he gave last ([`Check::of`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    issue: Issue,
    private: Vec<PrivateTerm>,
}

impl Check {
    /// What Rich gave when `draft` was put on the card: its words as the card shows them
    /// ([`sheet_of`]) and the report's private words.
    pub fn of(draft: &Draft) -> Check {
        Check { issue: issue_of(&sheet_of(draft)), private: draft.private.clone() }
    }

    /// The private words Rich named in this report.
    pub fn private(&self) -> &[PrivateTerm] {
        &self.private
    }
}

/// A new report's id, which its [`Check`] is kept under while its card is on screen.
pub fn report_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// **THE CARD'S WORDS, AS THE WINDOW READS THEM** (`sheetOf` in `ui/bug-report.js`): the title,
/// and each section's heading, paragraphs and steps, each one's words joined and trimmed, empty
/// ones dropped.
pub fn sheet_of(draft: &Draft) -> Sheet {
    let words = |segments: &Vec<Segment>| segments.iter().map(|s| s.text.as_str()).collect::<String>().trim().to_string();
    Sheet {
        title: words(&draft.title),
        sections: draft
            .sections
            .iter()
            .map(|s| SheetSection {
                heading: s.heading.trim().to_string(),
                paragraphs: s.paragraphs.iter().map(words).filter(|p| !p.is_empty()).collect(),
                steps: s.steps.iter().map(words).filter(|p| !p.is_empty()).collect(),
            })
            .collect(),
    }
}

/// What may happen when the user presses Send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtSend {
    /// These words go, exactly: the card's words when Send was pressed, which Rich gave last.
    Send(Public),
    /// The card's words were changed, went through the scanner and then Rich, and his version
    /// differs from the card: it is shown on the card (and is his check from then on), and nothing
    /// is sent until Send.
    Show(Checked),
    /// The card's words were changed and Claude could not answer (why): nothing is sent, and the
    /// words wait on this Mac ([`Outbox::keep_edited`]).
    Unchecked(String),
}

/// **THE ONE DECISION ABOUT WHAT LEAVES THIS MAC** (the order above). `sheet` is the card's words
/// at Send, `check` what Rich last gave of this report, `app` the private words RichOS holds,
/// `finish` one turn of Rich's. When the card's words are what he gave, to the character, they go
/// exactly. Otherwise they go through the scanner ([`redraft`]) and then Rich ([`finish_prompt`]):
/// when his version is the card's words exactly they go; otherwise it is shown first; when he
/// cannot answer, nothing goes.
pub fn at_send(sheet: &Sheet, check: Option<&Check>, app: &[PrivateTerm], finish: impl FnOnce(&str) -> Result<String, String>) -> AtSend {
    at_send_for(Post::Bug, sheet, check, app, finish)
}

/// [`at_send`] for a bug report or a feature idea: the one decision, the same for both.
pub fn at_send_for(post: Post, sheet: &Sheet, check: Option<&Check>, app: &[PrivateTerm], finish: impl FnOnce(&str) -> Result<String, String>) -> AtSend {
    let card = issue_of(sheet);
    if check.is_some_and(|c| c.issue == card) {
        return AtSend::Send(Public(card));
    }
    let private = check.map(|c| c.private.clone()).unwrap_or_default();
    match checked_edit_for(post, &Edited { sheet: sheet.clone(), private }, app, finish) {
        Err(why) => AtSend::Unchecked(why),
        Ok(his) if issue_of(&sheet_of(&his.draft)) == card => AtSend::Send(Public(card)),
        Ok(his) => AtSend::Show(his),
    }
}

/// **THE CARD'S WORDS AS A DRAFT AGAIN, THROUGH THE SCANNER**, with the report's private words
/// (`private`) and the ones RichOS holds: the title, each section's paragraphs as one string
/// ([`paragraphs_of`]) and each step, then the whole of it through [`as_posted`]. A stand-in
/// already on the card is plain words here (what it replaced is not in the sheet), and it stays
/// as it reads.
pub fn redraft(sheet: &Sheet, private: &[PrivateTerm], app: &[PrivateTerm]) -> Draft {
    let scrubber = Scrubber::with_rich(app.to_vec(), private.to_vec());
    let draft = Draft {
        title: scrubber.scrub(sheet.title.trim()),
        sections: sheet
            .sections
            .iter()
            .map(|s| DraftSection {
                heading: s.heading.clone(),
                paragraphs: paragraphs_of(&s.paragraphs.join("\n\n"), &scrubber),
                steps: s.steps.iter().map(|t| t.trim()).filter(|t| !t.is_empty()).map(|t| scrubber.scrub(t)).collect(),
            })
            .collect(),
        private: private.to_vec(),
    };
    as_posted(&draft, &scrubber)
}

/// The line over Rich's answer when he cleaned up words the user changed.
pub const EDIT_DIGEST: &str = "Checked your changes for private details";

/// **WORDS THE USER CHANGED THAT RICH COULD NOT CHECK YET**, kept on this Mac
/// ([`Outbox::keep_edited`]): the card's words when Send was pressed, and the report's private
/// words so far. Nothing of it is sent from there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edited {
    pub sheet: Sheet,
    pub private: Vec<PrivateTerm>,
}

/// **WORDS THE USER CHANGED, THROUGH THE SCANNER AND THEN RICH**: `edited` scanned ([`redraft`])
/// and given to Rich last (`finish`), as a card under [`EDIT_DIGEST`]. Like any check, it is
/// shown, never sent by itself.
pub fn checked_edit(edited: &Edited, app: &[PrivateTerm], finish: impl FnOnce(&str) -> Result<String, String>) -> Result<Checked, String> {
    checked_edit_for(Post::Bug, edited, app, finish)
}

/// [`checked_edit`] for a bug report or a feature idea.
pub fn checked_edit_for(post: Post, edited: &Edited, app: &[PrivateTerm], finish: impl FnOnce(&str) -> Result<String, String>) -> Result<Checked, String> {
    let scanned = redraft(&edited.sheet, &edited.private, app);
    Ok(Checked { draft: rich_last(post, &scanned, &as_typed(&edited.sheet), app, finish)?, digest: EDIT_DIGEST.into() })
}

// ---------------------------------------------------------------------------------------
// SEND, AND KEEP WHAT COULD NOT GO
// ---------------------------------------------------------------------------------------

/// Whose GitHub account an issue is filed from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Account {
    /// The user's own account, once RichOS can be signed in to GitHub (§20; not built yet).
    User { login: String },
    /// The RichOS reporting account, when RichOS is not signed in to one of theirs.
    Reporting,
}

/// A token and whose it is, read at send time and never stored by this module.
#[derive(Clone)]
pub struct Credential {
    pub account: Account,
    pub token: String,
}

impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credential").field("account", &self.account).field("token", &"<hidden>").finish()
    }
}

/// Who a report goes out as, asked at the moment of sending.
pub trait Credentials {
    /// `None` when there is no account to send from: the report waits ([`Reason::NotSetUp`]).
    fn credential(&self) -> Option<Credential>;
}

/// What came back from one attempt to create an issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// `201 Created`, with the issue's number and its page.
    Created { number: u64, url: String },
    /// Any other HTTP status.
    Status { code: u16 },
    /// No connection could be made: this Mac is offline, or cannot find GitHub.
    Unreachable,
    /// A connection was made and nothing came back in time.
    TimedOut,
}

/// The network calls, made by the shell: a bug report is one REST request to the issues
/// endpoint; a feature idea is GraphQL ([`post_discussion`]), never the issues endpoint.
pub trait Transport {
    fn create_issue(&self, credential: &Credential, path: &str, request: &serde_json::Value) -> Outcome;
    /// One GraphQL request (`POST /graphql`, `request` being `{query, variables}`), and GitHub's
    /// answer as it came: its HTTP status and its JSON.
    fn graphql(&self, credential: &Credential, request: &serde_json::Value) -> Reply;
    /// **AN IDEA, POSTED** as a discussion under Ideas: through GitHub's GraphQL API
    /// ([`post_discussion`]) unless the transport posts it in one step itself (the shell's RichOS
    /// reporting relay does).
    fn post_idea(&self, credential: &Credential, title: &str, body: &str) -> Outcome {
        post_discussion(self, credential, title, body)
    }
}

/// What came back from one GraphQL request.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    /// GitHub answered: its HTTP status, and its body as JSON (`null` when it was not JSON).
    Answered { status: u16, body: serde_json::Value },
    /// No connection could be made.
    Unreachable,
    /// A connection was made and nothing came back in time.
    TimedOut,
}

/// Finds the repository's id and its Ideas category's id, which `createDiscussion` takes.
pub const IDEAS_LOOKUP: &str =
    "query($owner: String!, $name: String!, $slug: String!) { repository(owner: $owner, name: $name) { id discussionCategory(slug: $slug) { id } } }";

/// Posts an idea: a discussion in the Ideas category, with the card's title and body.
pub const CREATE_DISCUSSION: &str = "mutation($repositoryId: ID!, $categoryId: ID!, $title: String!, $body: String!) { createDiscussion(input: {repositoryId: $repositoryId, categoryId: $categoryId, title: $title, body: $body}) { discussion { number url } } }";

/// **WHAT A GRAPHQL ANSWER MEANS, AS A BUG REPORT'S [`Outcome`]**, so an idea waits and is tried
/// again for exactly the reasons a bug report does ([`classify`]). Answers the answer's `data`
/// when there is some. GitHub answers a GraphQL request it refused with HTTP 200 and an `errors`
/// list: a token without the right to post in Discussions comes back `FORBIDDEN` or
/// `INSUFFICIENT_SCOPES`, a repository or category it will not show `NOT_FOUND`; each of those is
/// GitHub refusing the account ([`Reason::Refused`], as status 403). Being rate limited is GitHub
/// being busy (429). Any other error with no data is GitHub not answering properly (502).
fn graphql_outcome(reply: Reply) -> Result<serde_json::Value, Outcome> {
    let (status, body) = match reply {
        Reply::Unreachable => return Err(Outcome::Unreachable),
        Reply::TimedOut => return Err(Outcome::TimedOut),
        Reply::Answered { status, body } => (status, body),
    };
    if status != 200 {
        return Err(Outcome::Status { code: status });
    }
    let errors: Vec<String> =
        body["errors"].as_array().map(|e| e.iter().map(|e| e["type"].as_str().unwrap_or("").to_ascii_uppercase()).collect()).unwrap_or_default();
    if errors.iter().any(|t| matches!(t.as_str(), "FORBIDDEN" | "INSUFFICIENT_SCOPES" | "NOT_FOUND" | "UNAUTHORIZED")) {
        return Err(Outcome::Status { code: 403 });
    }
    if errors.iter().any(|t| t == "RATE_LIMITED") {
        return Err(Outcome::Status { code: 429 });
    }
    match body.get("data") {
        // Data with an error beside it is still read: a discussion that came back was posted, and
        // posting it again would post it twice.
        Some(data) if !data.is_null() => Ok(data.clone()),
        _ => Err(Outcome::Status { code: 502 }),
    }
}

/// **AN IDEA, POSTED**: a discussion in the Ideas category of [`REPOSITORY`], through GitHub's
/// GraphQL API, never the issues endpoint. Two requests: the repository's and the category's ids
/// ([`IDEAS_LOOKUP`]), then `createDiscussion` with them ([`CREATE_DISCUSSION`]). Answers a bug
/// report's [`Outcome`]: `Created` with the discussion's number and page, or why it waits. A
/// category that is not there is GitHub refusing it (403): there is nowhere to post an idea.
pub fn post_discussion<T: Transport + ?Sized>(transport: &T, credential: &Credential, title: &str, body: &str) -> Outcome {
    let (owner, name) = REPOSITORY.split_once('/').unwrap_or((REPOSITORY, ""));
    let lookup = serde_json::json!({ "query": IDEAS_LOOKUP, "variables": { "owner": owner, "name": name, "slug": IDEAS_CATEGORY_SLUG } });
    let data = match graphql_outcome(transport.graphql(credential, &lookup)) {
        Ok(data) => data,
        Err(outcome) => return outcome,
    };
    let (Some(repository), Some(category)) = (data["repository"]["id"].as_str(), data["repository"]["discussionCategory"]["id"].as_str()) else {
        return Outcome::Status { code: 403 };
    };
    let create = serde_json::json!({
        "query": CREATE_DISCUSSION,
        "variables": { "repositoryId": repository, "categoryId": category, "title": title, "body": body },
    });
    let data = match graphql_outcome(transport.graphql(credential, &create)) {
        Ok(data) => data,
        Err(outcome) => return outcome,
    };
    let discussion = &data["createDiscussion"]["discussion"];
    match discussion["number"].as_u64() {
        Some(number) => Outcome::Created { number, url: discussion["url"].as_str().map(str::to_string).unwrap_or_else(|| discussion_page(number)) },
        // No discussion came back: not a post we can point at, so it waits rather than claiming
        // it went up (as a 201 with no issue number does for a bug report).
        None => Outcome::Status { code: 502 },
    }
}

/// Why a report is waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    /// This Mac could not reach GitHub at all.
    Offline,
    /// GitHub did not answer, or answered that it is unavailable or busy.
    GithubDown,
    /// No account to send from.
    NotSetUp,
    /// GitHub refused the account (a token it does not accept, or a repository it will not file in).
    Refused,
}

impl Reason {
    /// How long a waiting report rests before it is tried again by itself. "Every few minutes"
    /// for GitHub (round 21); sooner for an offline Mac, whose attempt fails fast and costs nothing.
    pub fn retry_after_ms(self) -> u64 {
        match self {
            Reason::Offline => 30_000,
            Reason::GithubDown => 180_000,
            Reason::NotSetUp => 300_000,
            Reason::Refused => 600_000,
        }
    }
}

/// What GitHub's answer means: the new issue, or why the report waits.
pub fn classify(outcome: &Outcome) -> Result<(u64, String), Reason> {
    match outcome {
        Outcome::Created { number, url } => Ok((*number, url.clone())),
        Outcome::Unreachable => Err(Reason::Offline),
        Outcome::TimedOut => Err(Reason::GithubDown),
        Outcome::Status { code } if *code >= 500 || *code == 429 || (300..400).contains(code) => Err(Reason::GithubDown),
        Outcome::Status { .. } => Err(Reason::Refused),
    }
}

/// A report on this Mac that has not gone out yet, exactly as it was approved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    pub id: String,
    pub created_at_ms: u64,
    pub title: String,
    pub body: String,
    pub attempts: u32,
    pub reason: Option<Reason>,
    pub next_try_at_ms: u64,
}

/// A report that went out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sent {
    pub id: String,
    pub number: u64,
    pub url: String,
    pub account: Account,
    pub title: String,
    pub sent_at_ms: u64,
    /// A bug report (an issue) or a feature idea (a discussion under Ideas). A record kept before
    /// ideas existed is a bug report.
    #[serde(default)]
    pub post: Post,
}

/// What one attempt came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Delivery {
    Sent(Sent),
    Waiting { id: String, reason: Reason },
}

/// What canceling a waiting report came to ([`Outbox::withdraw`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Withdrawn {
    /// It is off this Mac and will never be sent.
    Canceled,
    /// It had already gone out: the issue it became.
    #[serde(rename = "sent")]
    AlreadySent(Sent),
}

/// Every `.json` file in `dir` that reads as a `T`; none when there is no such folder. A file this
/// build cannot read is skipped and left in place, never deleted.
fn read_all<T: serde::de::DeserializeOwned>(dir: &Path) -> io::Result<Vec<T>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e),
    };
    let mut out = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| serde_json::from_slice::<T>(&b).map_err(|e| e.to_string())) {
            Ok(item) => out.push(item),
            Err(e) => eprintln!("[richos] bug report: {} could not be read ({e}); left in place", path.display()),
        }
    }
    Ok(out)
}

/// The reports on this Mac: one file per waiting report in `<dir>/waiting/`, one line per sent
/// report in `<dir>/sent.jsonl`, and one file per report Rich has not checked yet in
/// `<dir>/unchecked/` (never sent from there: a report is sent only from a card the user approved).
///
/// **Callers serialize.** Two attempts at the same waiting report at once would file it twice,
/// so the shell holds one lock across every call into this store.
#[derive(Debug, Clone)]
pub struct Outbox {
    dir: PathBuf,
    /// What its posts are, and so where they go ([`Outbox::attempt`]).
    post: Post,
}

impl Outbox {
    /// Bug reports, kept in `dir`.
    pub fn open(dir: &Path) -> Self {
        Outbox::open_for(dir, Post::Bug)
    }

    /// Bug reports or feature ideas, kept in `dir`: one outbox per kind, each in its own folder,
    /// so a kept idea can only ever be posted as an idea.
    pub fn open_for(dir: &Path, post: Post) -> Self {
        Outbox { dir: dir.to_path_buf(), post }
    }

    /// What this outbox keeps: bug reports or feature ideas.
    pub fn post(&self) -> Post {
        self.post
    }

    fn waiting_dir(&self) -> PathBuf {
        self.dir.join("waiting")
    }

    fn path_of(&self, id: &str) -> Option<PathBuf> {
        // An id is ours (a uuid); anything else is refused rather than joined into a path.
        uuid::Uuid::parse_str(id).ok().map(|u| self.waiting_dir().join(format!("{u}.json")))
    }

    fn write(&self, pending: &Pending) -> io::Result<()> {
        let path = self.path_of(&pending.id).ok_or_else(|| io::Error::other("not a report id"))?;
        crate::quota::atomic_write(&path, pending)
    }

    /// Every waiting report, oldest first. A file this build cannot read is skipped, never deleted.
    pub fn pending(&self) -> io::Result<Vec<Pending>> {
        let mut out: Vec<Pending> = read_all(&self.waiting_dir())?;
        out.sort_by_key(|p| (p.created_at_ms, p.id.clone()));
        Ok(out)
    }

    fn unchecked_path(&self, id: &str) -> Option<PathBuf> {
        uuid::Uuid::parse_str(id).ok().map(|u| self.dir.join("unchecked").join(format!("{u}.json")))
    }

    fn write_unchecked(&self, unchecked: &Unchecked) -> io::Result<()> {
        let path = self.unchecked_path(&unchecked.id).ok_or_else(|| io::Error::other("not a report id"))?;
        crate::quota::atomic_write(&path, unchecked)
    }

    /// **KEPT UNTIL RICH HAS CHECKED IT.** His checked draft when there is one; otherwise the
    /// report is not offered for sending: the user's words and where they were are written to
    /// `<dir>/unchecked/` and wait for [`Outbox::check_due`]. Nothing of it can reach GitHub, which
    /// is sent only what the user approves on a card ([`Outbox::send`]).
    pub fn keep_unless_checked(&self, answer: &str, screen: &Screen, check: Result<Checked, String>, now_ms: u64) -> io::Result<WriteUp> {
        let why = match check {
            Ok(checked) => return Ok(WriteUp::Checked(checked)),
            Err(why) => why,
        };
        let unchecked = Unchecked {
            id: uuid::Uuid::new_v4().to_string(),
            created_at_ms: now_ms,
            answer: answer.trim().to_string(),
            screen: screen.clone(),
            attempts: 1,
            next_try_at_ms: now_ms.saturating_add(CHECK_RETRY_MS),
            checked: None,
            edited: None,
        };
        self.write_unchecked(&unchecked)?;
        eprintln!("[richos] bug report {}: Rich could not check it ({why}); it waits on this Mac", unchecked.id);
        Ok(WriteUp::Unchecked { id: unchecked.id })
    }

    /// **CHANGED WORDS RICH COULD NOT CHECK AT SEND**, kept on this Mac like any report he could
    /// not check (`why`): nothing of it is sent, and [`Outbox::check_due`] asks him about it again
    /// until he answers, when it comes back as a card ([`checked_edit`]) to send or not.
    pub fn keep_edited(&self, edited: Edited, screen: &Screen, why: &str, now_ms: u64) -> io::Result<WriteUp> {
        let unchecked = Unchecked {
            id: uuid::Uuid::new_v4().to_string(),
            created_at_ms: now_ms,
            answer: String::new(),
            screen: screen.clone(),
            attempts: 1,
            next_try_at_ms: now_ms.saturating_add(CHECK_RETRY_MS),
            checked: None,
            edited: Some(edited),
        };
        self.write_unchecked(&unchecked)?;
        eprintln!("[richos] bug report {}: Rich could not check the changed words ({why}); it waits on this Mac", unchecked.id);
        Ok(WriteUp::Unchecked { id: unchecked.id })
    }

    /// Every report Rich has not checked yet, or whose check the window has not taken, oldest
    /// first. A file this build cannot read is skipped, never deleted.
    pub fn unchecked(&self) -> io::Result<Vec<Unchecked>> {
        let mut out: Vec<Unchecked> = read_all(&self.dir.join("unchecked"))?;
        out.sort_by_key(|u| (u.created_at_ms, u.id.clone()));
        Ok(out)
    }

    /// **RICH IS ASKED AGAIN.** Each report whose rest is over is checked once (`check`, the shell's
    /// `claude` turn); one he still cannot check rests again ([`CHECK_RETRY_MS`]). Answers every
    /// report that is checked and not yet taken by the window, newly or earlier, so a window that
    /// missed the news is told again. A report taken back while he was being asked stays gone.
    pub fn check_due(&self, now_ms: u64, mut check: impl FnMut(&Unchecked) -> Result<Checked, String>) -> io::Result<Vec<(String, Checked)>> {
        let mut out = Vec::new();
        for mut unchecked in self.unchecked()? {
            if let Some(done) = unchecked.checked.clone() {
                out.push((unchecked.id, done));
                continue;
            }
            if unchecked.next_try_at_ms > now_ms {
                continue;
            }
            let answer = check(&unchecked);
            if !self.unchecked_path(&unchecked.id).is_some_and(|p| p.exists()) {
                continue;
            }
            match answer {
                Ok(done) => {
                    unchecked.checked = Some(done.clone());
                    self.write_unchecked(&unchecked)?;
                    out.push((unchecked.id, done));
                }
                Err(why) => {
                    eprintln!("[richos] bug report {}: Rich still could not check it ({why}); it waits on this Mac", unchecked.id);
                    unchecked.attempts += 1;
                    unchecked.next_try_at_ms = now_ms.saturating_add(CHECK_RETRY_MS);
                    self.write_unchecked(&unchecked)?;
                }
            }
        }
        Ok(out)
    }

    /// Take a report off the unchecked list: the window has shown Rich's check on a card, or the
    /// user canceled it. `false` when there was none by that id.
    pub fn take_unchecked(&self, id: &str) -> io::Result<bool> {
        let Some(path) = self.unchecked_path(id) else { return Ok(false) };
        match std::fs::remove_file(path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Every report that went out, oldest first.
    pub fn sent(&self) -> io::Result<Vec<Sent>> {
        let text = match std::fs::read_to_string(self.dir.join("sent.jsonl")) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(e),
        };
        Ok(text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect())
    }

    /// **CANCEL, CONFIRMED.** Take a waiting report off this Mac and say what became of it:
    /// [`Withdrawn::Canceled`] once its copy is gone (or there was none: nothing by that id will
    /// ever be sent), or [`Withdrawn::AlreadySent`] with the issue when it went out first. A copy
    /// that cannot be removed is an error, and the report still waits; the user is told so
    /// rather than that nothing was sent (second review finding 2).
    ///
    /// The shell calls this under the same one-at-a-time lock as every send, so a retry already
    /// in flight finishes first, and its record of going out is what this finds.
    pub fn withdraw(&self, id: &str) -> io::Result<Withdrawn> {
        if self.cancel(id)? {
            return Ok(Withdrawn::Canceled);
        }
        Ok(match self.sent()?.into_iter().find(|s| s.id == id) {
            Some(sent) => Withdrawn::AlreadySent(sent),
            None => Withdrawn::Canceled,
        })
    }

    /// Remove a waiting report. `false` when there was none by that id.
    pub fn cancel(&self, id: &str) -> io::Result<bool> {
        let Some(path) = self.path_of(id) else { return Ok(false) };
        match std::fs::remove_file(path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// **SEND WORDS THAT MAY LEAVE THIS MAC**, exactly ([`Public`], which only [`at_send`] makes).
    /// They are written to this Mac first, then tried once. Sent, or waiting with the reason;
    /// either way nothing approved is lost, and what is tried later is these words.
    pub fn send(&self, public: &Public, credentials: &dyn Credentials, transport: &dyn Transport, now_ms: u64) -> io::Result<Delivery> {
        let pending = Pending {
            id: uuid::Uuid::new_v4().to_string(),
            created_at_ms: now_ms,
            title: public.title().to_string(),
            body: public.body().to_string(),
            attempts: 0,
            reason: None,
            next_try_at_ms: now_ms,
        };
        self.write(&pending)?;
        self.attempt(pending, credentials, transport, now_ms)
    }

    /// *Try now*: one attempt at a waiting report, whenever it was due. `None` when it is not
    /// waiting (already sent, or canceled).
    pub fn try_now(&self, id: &str, credentials: &dyn Credentials, transport: &dyn Transport, now_ms: u64) -> io::Result<Option<Delivery>> {
        let Some(pending) = self.pending()?.into_iter().find(|p| p.id == id) else { return Ok(None) };
        self.attempt(pending, credentials, transport, now_ms).map(Some)
    }

    /// Every waiting report whose rest is over, tried once each, oldest first.
    pub fn send_due(&self, credentials: &dyn Credentials, transport: &dyn Transport, now_ms: u64) -> io::Result<Vec<Delivery>> {
        let mut out = Vec::new();
        for pending in self.pending()? {
            if pending.next_try_at_ms <= now_ms {
                out.push(self.attempt(pending, credentials, transport, now_ms)?);
            }
        }
        Ok(out)
    }

    fn attempt(&self, mut pending: Pending, credentials: &dyn Credentials, transport: &dyn Transport, now_ms: u64) -> io::Result<Delivery> {
        let outcome = match credentials.credential() {
            None => Err((Reason::NotSetUp, None)),
            Some(credential) => {
                // THE ONE DIFFERENCE BETWEEN THE TWO: where it is posted. A bug report is an issue;
                // a feature idea is a discussion under Ideas, and never reaches the issues endpoint
                // (CEO, 2026-10-11: "I don't want feature requests to be submitted or branded as
                // "issues" on GitHub").
                let answer = match self.post {
                    Post::Bug => {
                        let request = serde_json::json!({ "title": pending.title, "body": pending.body });
                        transport.create_issue(&credential, &format!("/repos/{REPOSITORY}/issues"), &request)
                    }
                    Post::Idea => transport.post_idea(&credential, &pending.title, &pending.body),
                };
                classify(&answer).map(|ok| (ok, credential.account)).map_err(|r| (r, Some(answer)))
            }
        };
        match outcome {
            Ok(((number, url), account)) => {
                let sent = Sent { id: pending.id.clone(), number, url, account, title: pending.title.clone(), sent_at_ms: now_ms, post: self.post };
                self.record_sent(&sent)?;
                Ok(Delivery::Sent(sent))
            }
            Err((reason, answer)) => {
                if let Some(answer) = answer {
                    eprintln!("[richos] bug report {}: not sent ({answer:?}); it waits on this Mac", pending.id);
                }
                pending.attempts += 1;
                pending.reason = Some(reason);
                pending.next_try_at_ms = now_ms.saturating_add(reason.retry_after_ms());
                self.write(&pending)?;
                Ok(Delivery::Waiting { id: pending.id, reason })
            }
        }
    }

    fn record_sent(&self, sent: &Sent) -> io::Result<()> {
        use io::Write;
        std::fs::create_dir_all(&self.dir)?;
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(self.dir.join("sent.jsonl"))?;
        file.write_all(format!("{}\n", serde_json::to_string(sent).map_err(io::Error::other)?).as_bytes())?;
        file.sync_all()?;
        // The record of it going out is on disk before the waiting copy goes, so a crash between
        // the two leaves a report both sent and waiting (filed twice at worst), never neither.
        self.cancel(&sent.id).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_takes_its_whole_line_and_keeps_the_full_stop_outside() {
        // Before review rv-20261009T154959Z-96dcc581-085b the closing bracket ended this path and
        // the quoted one was a second path; a bracket no longer ends one. Since review
        // rv-20261009T162841Z-69294215-70e6 the words before it on its line go too ("see").
        let text = "see (/Users/a/b.txt): and \"~/x\"";
        let found: Vec<&str> = find_paths(text).into_iter().map(|(s, e)| &text[s..e]).collect();
        assert_eq!(found, ["see (/Users/a/b.txt): and \"~/x\""]);
        // Since cc/echo-opus-bug7 left ". " ending a path inside a file's name, a sentence end
        // no longer ends one: the line's end does, and its final full stop stays outside.
        let text = "see /a. Then ~/x! And 10/09.";
        let found: Vec<&str> = find_paths(text).into_iter().map(|(s, e)| &text[s..e]).collect();
        assert_eq!(found, ["see /a. Then ~/x! And 10/09"]);
        let text = "see /a.\nThen ~/x!\r\nAnd 10/09.";
        let found: Vec<&str> = find_paths(text).into_iter().map(|(s, e)| &text[s..e]).collect();
        assert_eq!(found, ["see /a", "Then ~/x"]);
    }

    #[test]
    fn an_address_at_the_end_of_a_sentence_keeps_its_period_outside() {
        let text = "write to a.b+c@mail.example.com.";
        let found: Vec<&str> = find_emails(text).into_iter().map(|(s, e)| &text[s..e]).collect();
        assert_eq!(found, ["a.b+c@mail.example.com"]);
        assert!(find_emails("@home and x@y").is_empty());
    }

    #[test]
    fn non_ascii_text_around_a_name_is_handled_on_character_boundaries() {
        let scrubber = Scrubber::new(vec![PrivateTerm::new("Zoë Kim", Kind::PersonName)]);
        let segments = scrubber.scrub("“Zoë Kim” — café");
        let text: String = segments.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(text, "“[a person]” — café");
    }

    // ---- the review of 66ba53861 (rv-20261010T081721Z-66ba5386-a4d1), finding 1 ----

    /// What happened to one name in a generated report: the scanner hid it and Rich kept the
    /// stand-in, the scanner hid it and Rich put it back, it was a stand-in already in the words,
    /// Rich replaced it now (listing it), or it stayed as it was.
    #[derive(Clone, Copy, Debug)]
    enum Fate {
        Kept,
        PutBack,
        Already,
        Replaced,
        Stays,
    }

    /// One report, as the scanner left it (`scanned`) and as Rich answered (`answer`), with what
    /// each stand-in in his words truly replaced (`truth`, in order; `None` for one already there).
    struct Case {
        scanned: Draft,
        answer: String,
        truth: Vec<(Kind, Option<String>)>,
        about: String,
    }

    /// `paragraphs` of (words before, name, its kind, its fate), in one section, and Rich's list.
    fn case_of(paragraphs: &[Vec<(&str, &str, Kind, Fate)>], listed: &[(&str, Kind)]) -> Case {
        let mut scanned_paragraphs = Vec::new();
        let mut his_paragraphs = Vec::new();
        let mut truth = Vec::new();
        let mut about = String::new();
        for paragraph in paragraphs {
            let mut segments: Vec<Segment> = Vec::new();
            let mut his = String::new();
            let plain = |segments: &mut Vec<Segment>, words: &str| match segments.last_mut() {
                Some(last) if !last.is_stand_in() => last.text.push_str(words),
                _ => segments.push(Segment::plain(words)),
            };
            for &(before, name, kind, fate) in paragraph {
                plain(&mut segments, before);
                his.push_str(before);
                about.push_str(&format!("{before}<{name}:{fate:?}>"));
                match fate {
                    Fate::Kept => {
                        segments.push(Segment::stand_in(name, kind));
                        his.push_str(kind.stand_in());
                        truth.push((kind, Some(name.to_string())));
                    }
                    Fate::PutBack => {
                        segments.push(Segment::stand_in(name, kind));
                        his.push_str(name);
                    }
                    Fate::Already => {
                        plain(&mut segments, kind.stand_in());
                        his.push_str(kind.stand_in());
                        truth.push((kind, None));
                    }
                    Fate::Replaced => {
                        plain(&mut segments, name);
                        his.push_str(kind.stand_in());
                        truth.push((kind, Some(name.to_string())));
                    }
                    Fate::Stays => {
                        plain(&mut segments, name);
                        his.push_str(name);
                    }
                }
            }
            about.push_str(" | ");
            if segments.is_empty() {
                segments.push(Segment::plain(""));
            }
            scanned_paragraphs.push(segments);
            his_paragraphs.push(his.trim().to_string());
        }
        let rich_kind = |k: Kind| match k {
            Kind::PersonName => "person",
            Kind::CompanyName => "company",
            _ => "other",
        };
        let answer = serde_json::json!({
            "title": "Window froze",
            "sections": [{"heading": "What happened", "paragraphs": his_paragraphs, "steps": []}],
            "private": listed.iter().map(|&(t, k)| serde_json::json!({"text": t, "kind": rich_kind(k)})).collect::<Vec<_>>(),
        })
        .to_string();
        let scanned = Draft {
            title: vec![Segment::plain("Window froze")],
            sections: vec![DraftSection { heading: "What happened".into(), paragraphs: scanned_paragraphs, steps: vec![] }],
            private: vec![],
        };
        Case { scanned, answer, truth, about: format!("{about} listed {listed:?}") }
    }

    /// What each stand-in on `draft` says it replaced, in order, with its kind.
    fn receipts_on(draft: &Draft) -> Vec<(Kind, Option<String>)> {
        let mut all: Vec<&Segment> = draft.title.iter().collect();
        for section in &draft.sections {
            all.extend(section.paragraphs.iter().chain(&section.steps).flatten());
        }
        all.into_iter().filter_map(|s| s.kind.map(|k| (k, s.was.clone()))).collect()
    }

    /// **NO STAND-IN NAMES WORDS IT DID NOT REPLACE, AND DOUBT COVERS THE WHOLE KIND.** Each
    /// stand-in either names exactly what it replaced in its own place, or claims nothing; a kind
    /// with any stand-in that claims nothing has all of them claim nothing.
    fn holds(case: &Case) -> Result<bool, String> {
        let draft = finished(Ok(case.answer.clone()), &case.scanned)?;
        let got = receipts_on(&draft);
        if got.len() != case.truth.len() {
            return Err(format!("{} stand-ins on the card for {} in his words: {}", got.len(), case.truth.len(), case.about));
        }
        for ((kind, was), (true_kind, true_was)) in got.iter().zip(&case.truth) {
            if kind != true_kind || (was.is_some() && was != true_was) {
                return Err(format!("a stand-in names words it did not replace: got {got:?}, truly {:?}: {}", case.truth, case.about));
            }
        }
        for kind in Kind::ALL {
            let of_kind: Vec<_> = got.iter().zip(&case.truth).filter(|((k, _), _)| *k == kind).collect();
            let doubted = of_kind.iter().any(|((_, was), (_, true_was))| was != true_was);
            if doubted && of_kind.iter().any(|((_, was), _)| was.is_some()) {
                return Err(format!("{kind:?}: some stand-ins claim words while others of the kind are in doubt: {got:?}: {}", case.about));
            }
        }
        Ok(got == case.truth)
    }

    #[test]
    fn a_stand_in_names_what_it_replaced_only_where_that_is_certain() {
        use Fate::*;
        use Kind::{CompanyName as C, PersonName as P};
        // The review's fixture `overlapping-private-receipts.py`: "Mary Ann saw it. Bob saw it. Ann
        // saw it too.", all three replaced by Rich. On 66ba53861 "Ann" was found inside the
        // claimed "Mary Ann" (only start positions were reserved), and Bob's and Ann's receipts
        // swapped.
        let review = case_of(
            &[vec![("", "Mary Ann", P, Replaced), (" saw it. ", "Bob", P, Replaced), (" saw it. ", "Ann", P, Replaced), (" saw it too.", "", P, Stays)]],
            &[("Mary Ann", P), ("Bob", P), ("Ann", P)],
        );
        assert_eq!(holds(&review), Ok(true), "the review's case");
        // The same with Rich listing them in any order, and "Ann" listed before "Mary Ann".
        let reordered = case_of(
            &[vec![("", "Mary Ann", P, Replaced), (" saw it. ", "Bob", P, Replaced), (" saw it. ", "Ann", P, Replaced)]],
            &[("Ann", P), ("Bob", P), ("Mary Ann", P)],
        );
        assert_eq!(holds(&reordered), Ok(true), "listed in another order");
        // The earlier rounds' cases still name their own: an earlier stand-in claims nothing and
        // the scanner's names Alex; a name Rich cleans up before the scanner's names itself.
        let mixed = case_of(&[vec![("", "", P, Already), (" saw it. ", "Alex", P, Kept), (" saw it too.", "", P, Stays)]], &[]);
        assert_eq!(holds(&mixed), Ok(true), "the mixed edit");
        let before = case_of(&[vec![("", "Carol", P, Replaced), (" saw it. ", "Alex", P, Kept)]], &[("Carol", P)]);
        assert_eq!(holds(&before), Ok(true), "a name before the scanner's");
        // Rich hides a name and lists nothing for it: nothing ties his stand-ins to their
        // places, so none of the kind claims anything.
        let unlisted = case_of(&[vec![("", "Bob", P, Replaced), (" saw it. ", "Alex", P, Kept)]], &[]);
        let draft = finished(Ok(unlisted.answer.clone()), &unlisted.scanned).unwrap();
        assert_eq!(receipts_on(&draft), [(P, None), (P, None)], "an unlisted stand-in");
        // He changes a word beside them: the words no longer reconcile, so none claims anything.
        let mut changed = case_of(&[vec![("", "Bob", P, Kept), (" saw it. ", "Acme", C, Replaced)]], &[("Acme", C)]);
        changed.answer = changed.answer.replace("saw it", "said it");
        let draft = finished(Ok(changed.answer.clone()), &changed.scanned).unwrap();
        assert_eq!(receipts_on(&draft), [(P, None), (C, None)], "words that do not reconcile");
    }

    #[test]
    fn generated_reports_never_give_a_stand_in_words_it_did_not_replace() {
        use Fate::*;
        use Kind::{CompanyName as C, PersonName as P};
        // GENERATED MIXES: repeated, nested and overlapping names, glued to each other or apart,
        // across paragraphs, each hidden by the scanner and kept or put back, already a stand-in,
        // replaced by Rich, or left; his list in any order, with repeats and with names he did
        // not use. Every one must hold, and the plain ones must be exact.
        let names: [(&str, Kind); 10] = [
            ("Ann", P),
            ("Mary Ann", P),
            ("Anna", P),
            ("Mary", P),
            ("Ann Lee", P),
            ("Bob", P),
            ("Bobby", P),
            ("Acme", C),
            ("Acme Corp", C),
            ("Corp", C),
        ];
        let between = ["", " ", " saw it. ", " and ", ", ", "\n", "  "];
        let fates = [Kept, PutBack, Already, Replaced, Stays];
        let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        let (mut exact, mut doubted) = (0, 0);
        for _ in 0..3000 {
            let mut paragraphs = Vec::new();
            let mut used: Vec<(&str, Kind)> = Vec::new();
            for _ in 0..1 + next(3) {
                let mut paragraph = Vec::new();
                for _ in 0..1 + next(6) {
                    let (name, kind) = names[next(names.len())];
                    let fate = fates[next(fates.len())];
                    if let Replaced = fate {
                        used.push((name, kind));
                    }
                    paragraph.push((between[next(between.len())], name, kind, fate));
                }
                paragraph.push((between[next(between.len())], "", P, Stays));
                paragraphs.push(paragraph);
            }
            // His list: what he replaced, each once or twice, in any order, sometimes with a name
            // he did not replace.
            let mut listed: Vec<(&str, Kind)> = Vec::new();
            for &u in &used {
                if !listed.contains(&u) || next(4) == 0 {
                    listed.push(u);
                }
            }
            if next(3) == 0 {
                listed.push(names[next(names.len())]);
            }
            for i in (1..listed.len()).rev() {
                listed.swap(i, next(i + 1));
            }
            let case = case_of(&paragraphs, &listed);
            match holds(&case) {
                Ok(true) => exact += 1,
                Ok(false) => {
                    doubted += 1;
                    println!("in doubt: {}", case.about.replace('\n', "\\n"));
                }
                Err(why) => panic!("{why}"),
            }
        }
        // Doubt is rare among reports that follow the rules; it is the honest answer only where
        // the words really do not tell (measured, not assumed: the count is printed).
        println!("generated: {exact} exact, {doubted} in doubt");
        assert!(exact > 2500, "only {exact} of 3000 generated reports were exact ({doubted} in doubt)");
    }
}
