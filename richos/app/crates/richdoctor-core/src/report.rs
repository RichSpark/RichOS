//! **THE REPORT IS THE EXACT OUTGOING TEXT** (PRD §2.4, §5; round 39 §4).
//!
//! A [`Report`] is what the review screen shows, section by section, every section editable;
//! [`Report::render`] writes the exact title and body that will be sent, and the review's
//! "Show as it will be sent" shows those same bytes. The body is Markdown written by this file
//! only: `##` headings in round 39's order, excerpts quoted under their source line, the footer
//! last. Every word that came from a person, a log or Claude goes through
//! [`richos_core::bug_report::escaped_lines`] (Bust a bug's own escaping), so GitHub shows the
//! words and no link, image, mention or formatting hides in them.
//!
//! The size budget is applied BEFORE review (PRD §5): at most 60,000 UTF-8 bytes of body,
//! footer and escaping included, and a 200-character title. Symptom and versions are kept
//! first; excerpts are shortened from their oldest lines, then left out, each with a plain note.
use crate::adapter::Drafted;
use crate::evidence::{Item, Removed, Status};
use crate::scrub::{self, Context, BODY_MAX_BYTES, TITLE_MAX_CHARS};
use richos_core::bug_report::escaped_lines;
use serde::{Deserialize, Serialize};

/// The footer line, the body's last line, exactly as the relay's reconciliation matches it.
pub fn footer(report_id: &str) -> String {
    format!("Sent with RichDoctor {} · Report ID {report_id}", crate::footer_version())
}

/// **THE BODY'S LAST LINES** (relay protocol 2, byte for byte; round 40's wording): with `k`
/// screenshots, "Screenshots: k, shared privately with RichOS support (staff sign-in required)"
/// and one numbered staff-only link per image (PRD §6: "numbered support-only links to each stored
/// image, tied to the report ID"), then the footer. With none, the footer alone. Written by this
/// file only, never escaped, so the relay finds exactly these bytes.
pub fn ending(relay: &str, report_id: &str, k: usize) -> String {
    let mut out = String::new();
    if k > 0 {
        out.push_str(&format!("Screenshots: {k}, shared privately with RichOS support (staff sign-in required)\n"));
        for n in 1..=k {
            out.push_str(&format!("{n}. {}\n", crate::delivery::attachment_link(relay, report_id, n as u32)));
        }
    }
    out.push_str(&footer(report_id));
    out
}

/// The gap RichDoctor can never fill, named rather than reconstructed (PRD §5).
pub const RUNTIME_LOG_GAP: &str = "RichOS's own runtime log is not written when RichOS is opened from Finder, the Dock or Spotlight, so it is not included.";

/// One quoted piece of evidence under "What RichDoctor found".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Excerpt {
    pub evidence_id: String,
    /// "Startup log · 8:41 and 8:43 AM".
    pub source_line: String,
    pub text: String,
    /// The person removed it: not sent, and "Put back" restores it.
    #[serde(default)]
    pub removed: bool,
}

/// Where the report's words came from, for the review's note (not part of the body).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provenance {
    Claude,
    WithoutAi,
    AiFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub report_id: String,
    pub title: String,
    pub what_happened: String,
    pub expected: String,
    pub when: String,
    pub steps: Vec<String>,
    pub versions: String,
    pub excerpts: Vec<Excerpt>,
    /// The approved screenshots that go with this report, by their local IDs
    /// ([`crate::store::Shot`]), in order. PRD version 6 (C3): they are delivered privately to
    /// RichOS support with the report, and the body carries their count and staff-only links
    /// ([`ending`]).
    #[serde(default)]
    pub screenshots: Vec<String>,
    /// The reporting relay's base the screenshot links are built from.
    #[serde(default)]
    pub relay: String,
    pub causes: String,
    pub missing: Vec<String>,
    pub provenance: Provenance,
    /// What the automatic scrub removed, with the source it was removed from.
    #[serde(default)]
    pub removals: Vec<(String, Removed)>,
    /// Plain notes on what was shortened to fit the size limit.
    #[serde(default)]
    pub shortened: Vec<String>,
}

/// The section keys the window edits by.
pub const SECTIONS: [&str; 7] = ["title", "what_happened", "expected", "when", "versions", "causes", "missing"];

/// Everything a report is built from.
pub struct Input<'a> {
    pub report_id: &'a str,
    pub description: &'a str,
    /// The conversation's questions and answers, in order; `None` is not answered.
    pub answers: &'a [(String, Option<String>)],
    pub when: &'a str,
    pub evidence: &'a [Item],
    pub drafted: Option<&'a Drafted>,
    pub provenance: Provenance,
    /// The draft's approved screenshots, by local ID.
    pub screenshots: &'a [String],
    /// The reporting relay's base (the screenshot links' origin).
    pub relay: &'a str,
    pub ctx: &'a Context,
}

fn bounded(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn first_sentence_title(description: &str) -> String {
    let d = description.trim();
    let end = d.find(['.', '!', '?', '\n']).unwrap_or(d.len());
    let t = d[..end].trim();
    if t.is_empty() { "RichOS problem report".into() } else { bounded(t, 90) }
}

impl Report {
    /// **BUILD THE REPORT** from the person's words, the evidence and, when there is one,
    /// Claude's draft. Host code decides the structure; Claude's words are scrubbed like the
    /// person's; evidence is quoted from the sanitized items, never from Claude.
    pub fn build(input: &Input) -> Report {
        let mut removals: Vec<(String, Removed)> = Vec::new();
        let mut clean = |text: &str, source: &str| -> String {
            let (out, removed) = scrub::scrub(text, input.ctx);
            if removed.total() > 0 {
                removals.push((source.to_string(), removed));
            }
            out
        };
        let d = input.drafted;
        let title = clean(&bounded(d.map(|d| d.title.as_str()).filter(|t| !t.is_empty()).unwrap_or(&first_sentence_title(input.description)), TITLE_MAX_CHARS), "title");
        let what = d.map(|d| d.what_happened.clone()).filter(|t| !t.trim().is_empty()).unwrap_or_else(|| input.description.trim().to_string());
        let mut what_happened = clean(&what, "your description");
        if d.is_some() && !what.contains(input.description.trim()) {
            // The person's own words stay in the report, beside Claude's summary of them.
            let own = clean(input.description.trim(), "your description");
            what_happened = format!("{what_happened}\n\nIn the person's words: \"{own}\"");
        }
        // Without Claude's draft, the built-in questions' answers fill their own sections: what
        // the person expected, and what they were doing just before (the steps).
        let answered = |q: &str| input.answers.iter().find(|(question, _)| question == q).and_then(|(_, a)| a.as_deref()).map(str::trim).filter(|a| !a.is_empty());
        let expected = match d {
            Some(d) => clean(&d.expected, "your answers"),
            None => clean(answered(WITHOUT_AI_QUESTIONS[1]).unwrap_or(""), "your answers"),
        };
        let when = clean(if d.is_some_and(|d| !d.when.is_empty()) { &d.unwrap().when } else { input.when }, "your answers");
        let steps: Vec<String> = match d {
            Some(d) => d.steps.iter().map(|s| clean(s, "your answers")).collect(),
            None => answered(WITHOUT_AI_QUESTIONS[2]).map(|a| vec![format!("Just before it happened: {}", clean(a, "your answers"))]).unwrap_or_default(),
        };

        let installation = input.evidence.iter().find(|i| i.category == crate::collect::Category::Installation && i.status.has_text());
        let versions = installation.map(|i| i.text.clone()).unwrap_or_else(|| format!("RichDoctor {}\nRichOS: the installation could not be checked.", crate::VERSION));

        let wanted: Option<&Vec<String>> = d.map(|d| &d.evidence_used).filter(|ids| !ids.is_empty());
        let excerpts: Vec<Excerpt> = input
            .evidence
            .iter()
            .filter(|i| i.status.has_text() && !i.text.trim().is_empty() && i.category != crate::collect::Category::Installation)
            .filter(|i| wanted.is_none_or(|ids| ids.contains(&i.id)))
            .map(|i| {
                let when = i.observed_at.map(|t| format!(" · {}", crate::util::local_clock(t))).unwrap_or_default();
                Excerpt { evidence_id: i.id.clone(), source_line: format!("{}{when}", i.source), text: i.text.clone(), removed: false }
            })
            .collect();

        let quoted: Vec<(String, Removed)> = excerpts
            .iter()
            .filter_map(|e| input.evidence.iter().find(|i| i.id == e.evidence_id))
            .filter(|i| i.removed.total() > 0)
            .map(|i| (i.source.to_lowercase(), i.removed.clone()))
            .collect();
        let causes = match (input.provenance, d) {
            (_, Some(d)) if !d.possible_causes.is_empty() => d
                .possible_causes
                .iter()
                .map(|c| {
                    let sources: Vec<String> = c.evidence.iter().filter_map(|id| input.evidence.iter().find(|i| &i.id == id)).map(|i| i.source.to_lowercase()).collect();
                    let basis = if sources.is_empty() { " (a guess; no evidence names it)".to_string() } else { format!(" (from the {})", sources.join(" and the ")) };
                    format!("{}{basis}", clean(&c.text, "Claude's draft"))
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
            (Provenance::Claude, _) => "No possible cause stood out in what was collected.".into(),
            _ => "Not assessed: this report was prepared without AI. The engineers will read the evidence above.".into(),
        };

        let mut missing: Vec<String> = Vec::new();
        for item in input.evidence.iter().filter(|i| !i.status.has_text() || !i.note.is_empty()) {
            if !item.note.is_empty() && !missing.contains(&item.note) {
                missing.push(item.note.clone());
            }
        }
        missing.push(RUNTIME_LOG_GAP.into());
        if let Some(d) = d {
            for m in &d.missing {
                missing.push(clean(m, "Claude's draft"));
            }
        }
        for (question, answer) in input.answers {
            if answer.as_deref().is_none_or(|a| a.trim().is_empty()) {
                missing.push(format!("Not answered, unknown: {}", clean(question, "your answers")));
            }
        }
        if input.evidence.is_empty() {
            missing.insert(0, "No diagnostic information was collected from this Mac: the person chose to describe the problem only.".into());
        }
        removals.extend(quoted);
        let mut report = Report {
            report_id: input.report_id.to_string(),
            title,
            what_happened,
            expected,
            when: if when.trim().is_empty() { "Not known.".into() } else { when },
            steps,
            versions,
            excerpts,
            screenshots: input.screenshots.to_vec(),
            relay: input.relay.to_string(),
            causes,
            missing,
            provenance: input.provenance,
            removals,
            shortened: Vec::new(),
        };
        report.fit();
        report
    }

    /// Replace one section with the person's edit, scrubbed like everything else, and fit the
    /// size budget again. `versions` and the excerpts are evidence: they can be removed or
    /// shortened, never rewritten into something the evidence did not say.
    /// **WHAT HAPPENED, AS IT IS SENT**: the description, then "Expected: ..." and the numbered
    /// steps when there are any. The review shows this one text, so everything in it can be
    /// changed or removed there ([`Report::edit`] splits it back).
    pub fn what_happened_view(&self) -> String {
        let mut what = self.what_happened.clone();
        if !self.expected.trim().is_empty() {
            what.push_str(&format!("\n\nExpected: {}", self.expected.trim()));
        }
        if !self.steps.is_empty() {
            what.push_str("\n\nSteps the person described:");
            for (n, s) in self.steps.iter().enumerate() {
                what.push_str(&format!("\n{}. {}", n + 1, s.trim()));
            }
        }
        what
    }

    /// Split an edited [`Report::what_happened_view`] back into the description, Expected and the
    /// steps. A part the person deleted is gone; one they kept or changed is kept as edited.
    fn set_what_happened(&mut self, text: &str) {
        let mut rest = text.trim_end().to_string();
        self.steps = Vec::new();
        if let Some(at) = rest.find("Steps the person described:") {
            let block = rest[at + "Steps the person described:".len()..].to_string();
            self.steps = block
                .lines()
                .map(|l| {
                    let l = l.trim();
                    let digits = l.chars().take_while(char::is_ascii_digit).count();
                    if digits > 0 && l[digits..].starts_with('.') { l[digits + 1..].trim().to_string() } else { l.to_string() }
                })
                .filter(|l| !l.is_empty())
                .collect();
            rest.truncate(at);
        }
        self.expected = String::new();
        if let Some(at) = rest.rfind("Expected:") {
            if at == 0 || rest[..at].ends_with('\n') {
                self.expected = rest[at + "Expected:".len()..].trim().to_string();
                rest.truncate(at);
            }
        }
        self.what_happened = rest.trim_end().to_string();
    }

    pub fn edit(&mut self, section: &str, text: &str, ctx: &Context) -> Result<(), String> {
        let (clean, removed) = scrub::scrub(text, ctx);
        if removed.total() > 0 {
            self.removals.push(("your edit".into(), removed));
        }
        match section {
            "title" => self.title = bounded(clean.trim(), TITLE_MAX_CHARS),
            "what_happened" => self.set_what_happened(&clean),
            "expected" => self.expected = clean,
            "when" => self.when = clean,
            "versions" => self.versions = clean,
            "causes" => self.causes = clean,
            "missing" => self.missing = clean.lines().map(|l| l.trim().trim_start_matches(['-', '•']).trim().to_string()).filter(|l| !l.is_empty()).collect(),
            _ => return Err(format!("no section named {section}")),
        }
        self.fit();
        Ok(())
    }

    /// Remove an excerpt, or put it back.
    pub fn set_excerpt_removed(&mut self, evidence_id: &str, removed: bool) {
        if let Some(e) = self.excerpts.iter_mut().find(|e| e.evidence_id == evidence_id) {
            e.removed = removed;
        }
        self.fit();
    }

    /// **THE EXACT OUTGOING TITLE AND BODY.** The same bytes the review shows as "Show as it
    /// will be sent", hashed for the relay and frozen at approval.
    pub fn render(&self) -> (String, String) {
        let mut b = String::new();
        let section = |b: &mut String, heading: &str, text: &str| {
            if !b.is_empty() {
                b.push('\n');
            }
            b.push_str("## ");
            b.push_str(heading);
            b.push_str("\n\n");
            for (i, para) in text.split("\n\n").map(str::trim).filter(|p| !p.is_empty()).enumerate() {
                if i > 0 {
                    b.push('\n');
                }
                b.push_str(&escaped_lines(para));
                b.push('\n');
            }
        };
        let mut what = self.what_happened.clone();
        if !self.expected.trim().is_empty() {
            what.push_str(&format!("\n\nExpected: {}", self.expected.trim()));
        }
        if !self.steps.is_empty() {
            what.push_str("\n\nSteps the person described:");
        }
        section(&mut b, "What happened", &what);
        if !self.steps.is_empty() {
            for (n, s) in self.steps.iter().enumerate() {
                b.push_str(&format!("{}. {}\n", n + 1, escaped_lines(s.trim()).replace('\n', " ")));
            }
        }
        section(&mut b, "When", &self.when);
        section(&mut b, "Versions", &self.versions);
        let shown: Vec<&Excerpt> = self.excerpts.iter().filter(|e| !e.removed).collect();
        b.push_str("\n## What RichDoctor found\n\n");
        if shown.is_empty() {
            b.push_str("No diagnostic excerpts are included.\n");
        }
        for (i, e) in shown.iter().enumerate() {
            if i > 0 {
                b.push('\n');
            }
            b.push_str(&escaped_lines(&e.source_line));
            b.push('\n');
            for line in escaped_lines(e.text.trim_end()).split('\n') {
                b.push_str("> ");
                b.push_str(line);
                b.push('\n');
            }
        }
        section(&mut b, "Possible cause (not confirmed)", &self.causes);
        b.push_str("\n## Missing information\n\n");
        for m in &self.missing {
            b.push_str("- ");
            b.push_str(&escaped_lines(m.trim()).replace('\n', " "));
            b.push('\n');
        }
        b.push('\n');
        b.push_str(&ending(&self.relay, &self.report_id, self.screenshots.len()));
        (self.title.clone(), b)
    }

    /// The exact text as the review's "Show as it will be sent" block: title, a blank line,
    /// body. Its bytes are what the relay's `content_sha256` is taken over.
    pub fn exact(&self) -> String {
        let (t, b) = self.render();
        format!("{t}\n\n{b}")
    }

    /// What the last mechanical check finds in the rendered text ([`scrub::check_outgoing`]).
    pub fn problems(&self) -> Vec<scrub::Problem> {
        let (t, b) = self.render();
        scrub::check_outgoing(&t, &b, None)
    }

    /// The review's note on what was removed automatically: "1 file path (startup log), 1
    /// email address (crash record), shown as [path removed] and [email removed]."
    pub fn removal_note(&self) -> Option<String> {
        let mut parts = Vec::new();
        let mut shown = Vec::new();
        for (source, r) in &self.removals {
            for (n, one, many, stand) in [
                (r.paths, "file path", "file paths", scrub::PATH),
                (r.emails, "email address", "email addresses", scrub::EMAIL),
                (r.secrets, "password or access key", "passwords or access keys", scrub::SECRET),
                (r.names, "name", "names", scrub::NAME),
            ] {
                if n > 0 {
                    parts.push(format!("{n} {} ({source})", if n == 1 { one } else { many }));
                    if !shown.contains(&stand) {
                        shown.push(stand);
                    }
                }
            }
        }
        if parts.is_empty() {
            return None;
        }
        let shown = match shown.as_slice() {
            [one] => one.to_string(),
            [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
            [] => String::new(),
        };
        Some(format!("Removed automatically before this review: {}, shown as {shown}. Remove anything else private.", parts.join(", ")))
    }

    /// **FIT THE SIZE BUDGET** (PRD §5): shorten the largest excerpt to its last lines, then
    /// leave excerpts out, then shorten the description, until the body fits; every step leaves
    /// a plain note. Symptom, versions and the footer are kept first.
    pub fn fit(&mut self) {
        self.title = bounded(&self.title, TITLE_MAX_CHARS);
        self.shortened.clear();
        let size = |r: &Report| r.render().1.len();
        while size(self) > BODY_MAX_BYTES {
            let largest = self.excerpts.iter_mut().filter(|e| !e.removed && e.text.lines().count() > 3).max_by_key(|e| e.text.len());
            if let Some(e) = largest {
                let lines: Vec<&str> = e.text.lines().collect();
                let keep = (lines.len() / 2).max(3);
                e.text = lines[lines.len() - keep..].join("\n");
                let note = format!("Shortened to fit the size limit: the {} excerpt was cut to its last {keep} lines. The full sanitized packet stays on this Mac and is not sent.", e.source_line.split(" · ").next().unwrap_or("").to_lowercase());
                self.shortened.retain(|n| !n.contains(&e.source_line.split(" · ").next().unwrap_or("").to_lowercase()));
                self.shortened.push(note);
                continue;
            }
            if let Some(e) = self.excerpts.iter_mut().rev().find(|e| !e.removed) {
                e.removed = true;
                self.shortened.push(format!("Left out to fit the size limit: the {} excerpt. It stays on this Mac and is not sent.", e.source_line.split(" · ").next().unwrap_or("").to_lowercase()));
                continue;
            }
            let over = size(self) - BODY_MAX_BYTES;
            let keep = self.what_happened.len().saturating_sub(over + 200);
            let cut: String = self.what_happened.char_indices().take_while(|(i, _)| *i < keep).map(|(_, c)| c).collect();
            if cut.len() >= self.what_happened.len() || cut.is_empty() {
                self.missing.truncate(5);
                self.causes = bounded(&self.causes, 2000);
                self.versions = bounded(&self.versions, 2000);
                self.when = bounded(&self.when, 500);
                self.expected = bounded(&self.expected, 1000);
                if size(self) > BODY_MAX_BYTES {
                    self.what_happened = bounded(&self.what_happened, 20_000);
                }
                if size(self) > BODY_MAX_BYTES {
                    self.what_happened = bounded(&self.what_happened, 2_000);
                }
                self.shortened.push("Shortened to fit the size limit: some long sections were cut.".into());
                break;
            }
            self.what_happened = format!("{cut}…");
            self.shortened.push("Shortened to fit the size limit: the description was cut.".into());
        }
    }
}

/// The no-AI path's questions (round 39: "Questions Claude would have asked are listed as
/// unknown so you can fill them in").
pub const WITHOUT_AI_QUESTIONS: [&str; 3] = ["About what time did it happen?", "What did you expect to happen?", "What were you doing just before it happened?"];

/// Whether an item belongs in the no-AI path's default collection.
pub fn without_ai_categories(diagnostics: bool) -> Vec<crate::collect::Category> {
    use crate::collect::Category::*;
    if diagnostics { vec![Installation, StartupCrash, ConversationFailure, Richscribe, ClaudeAvailability] } else { vec![Installation] }
}

/// An item's status in the review's words, for the found-section header.
pub fn status_words(status: Status) -> &'static str {
    match status {
        Status::Collected => "collected",
        Status::Truncated => "shortened",
        Status::Absent => "not on this Mac",
        Status::Denied => "not allowed by macOS",
        Status::Unreadable => "unreadable",
        Status::Unsupported => "in a form RichDoctor cannot read safely",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_given_without_ai_are_kept_in_the_report() {
        let ctx = Context::with("jdoe".into(), vec![]);
        let answers: Vec<(String, Option<String>)> = vec![
            (WITHOUT_AI_QUESTIONS[0].into(), Some("Earlier today".into())),
            (WITHOUT_AI_QUESTIONS[1].into(), Some("The window opens with my conversations".into())),
            (WITHOUT_AI_QUESTIONS[2].into(), Some("I had just updated macOS".into())),
        ];
        let r = Report::build(&Input {
            report_id: "RD-7K3M-Q9VX",
            description: "RichOS closes before its window opens.",
            answers: &answers,
            when: "Earlier today",
            evidence: &[],
            drafted: None,
            provenance: Provenance::WithoutAi,
            screenshots: &[],
            relay: "https://report.richos.ceo",
            ctx: &ctx,
        });
        assert!(r.expected.contains("The window opens with my conversations"), "{:?}", r.expected);
        assert!(r.steps.iter().any(|s| s.contains("I had just updated macOS")), "{:?}", r.steps);
        let (_, body) = r.render();
        assert!(body.contains("I had just updated macOS"));
    }

    #[test]
    fn a_privacy_placeholder_in_a_plain_title_can_be_sent() {
        let ctx = Context::with("jdoe".into(), vec!["SecretCo".into()]);
        let r = Report::build(&Input {
            report_id: "RD-7K3M-Q9VX",
            description: "RichOS closes when I open SecretCo.",
            answers: &[],
            when: "Earlier today",
            evidence: &[],
            drafted: None,
            provenance: Provenance::WithoutAi,
            screenshots: &[],
            relay: "https://report.richos.ceo",
            ctx: &ctx,
        });
        let (title, body) = r.render();
        assert!(title.contains("[name removed]") && !title.contains("SecretCo"), "{title}");
        let problems = scrub::check_outgoing(&title, &body, None);
        assert!(problems.is_empty(), "{problems:?}");
        // The body's markup rule still holds: an unescaped bracket in the body is refused.
        assert!(!scrub::check_outgoing("t", "a [link](x)", None).is_empty());
    }
    use crate::collect::Category;
    use crate::evidence::Item;

    fn item(category: Category, source: &str, text: &str) -> Item {
        let mut i = Item::new(category, source).unwrap();
        i.status = Status::Collected;
        i.text = text.into();
        i
    }

    fn sample(evidence: &[Item], drafted: Option<&Drafted>) -> Report {
        let ctx = Context::with("jdoe".into(), vec!["Jane Doe".into()]);
        let answers = vec![("About what time did it happen?".to_string(), Some("Earlier today".to_string())), ("What did you expect?".to_string(), None)];
        Report::build(&Input {
            report_id: "RD-7K3M-Q9VX",
            description: "RichOS closes before its window opens. I tried twice this morning. Jane Doe here, at /Users/jdoe/Desktop.",
            answers: &answers,
            when: "Earlier today, about 8:40 AM (UTC-07:00)",
            evidence,
            drafted,
            provenance: if drafted.is_some() { Provenance::Claude } else { Provenance::WithoutAi },
            screenshots: &[],
            relay: "https://report.richos.ceo",
            ctx: &ctx,
        })
    }

    #[test]
    fn the_body_has_round_39s_sections_in_order_and_the_footer_last() {
        let ev = [item(Category::Installation, "Installation", "RichDoctor 1.0.0\nRichOS 1.2.1 in Applications"), item(Category::StartupCrash, "Startup log", "failure: the engine did not start\n- a line that starts with a dash")];
        let r = sample(&ev, None);
        let (title, body) = r.render();
        assert_eq!(title, "RichOS closes before its window opens");
        let order = ["## What happened", "## When", "## Versions", "## What RichDoctor found", "## Possible cause (not confirmed)", "## Missing information"];
        let mut at = 0;
        for h in order {
            let i = body[at..].find(h).unwrap_or_else(|| panic!("{h} missing or out of order:\n{body}")) + at;
            at = i;
        }
        assert!(body.ends_with("Sent with RichDoctor 1.0 · Report ID RD-7K3M-Q9VX"), "{body}");
        assert!(body.contains("> \\- a line that starts with a dash"), "excerpt lines are quoted and escaped:\n{body}");
        assert!(body.contains("Not answered, unknown: What did you expect?"), "{body}");
        assert!(body.contains(RUNTIME_LOG_GAP));
        assert!(!body.contains("Jane") && !body.contains("/Users/jdoe"), "the description is scrubbed:\n{body}");
        assert!(body.contains("\\[path removed\\]") && body.contains("\\[name removed\\]"));
        assert_eq!(r.problems(), vec![], "a built report passes the last check");
        assert!(r.removal_note().unwrap().starts_with("Removed automatically before this review: 1 file path (your description), 1 name (your description)"), "{:?}", r.removal_note());
    }

    #[test]
    fn missing_evidence_reads_as_missing_never_as_no_errors() {
        let mut absent = Item::new(Category::StartupCrash, "Crash record").unwrap();
        absent.note = "No crash record for RichOS was on this Mac.".into();
        let r = sample(&[absent], None);
        let (_, body) = r.render();
        assert!(body.contains("- No crash record for RichOS was on this Mac."), "{body}");
        assert!(body.contains("No diagnostic excerpts are included."));
        assert!(body.contains("Not assessed: this report was prepared without AI."));
    }

    #[test]
    fn claudes_draft_is_scrubbed_and_causes_name_their_evidence() {
        let ev = [item(Category::StartupCrash, "Crash record", "Exception: EXC_CRASH SIGABRT")];
        let d = Drafted {
            title: "RichOS quits at launch for Jane Doe".into(),
            what_happened: "RichOS closes before its window opens.".into(),
            possible_causes: vec![crate::adapter::Cause { text: "An abort during startup".into(), evidence: vec![ev[0].id.clone()] }, crate::adapter::Cause { text: "A damaged install".into(), evidence: vec![] }],
            evidence_used: vec![ev[0].id.clone()],
            ..Default::default()
        };
        let r = sample(&ev, Some(&d));
        assert_eq!(r.title, "RichOS quits at launch for [name removed]");
        assert!(r.causes.contains("An abort during startup (from the crash record)") && r.causes.contains("A damaged install (a guess; no evidence names it)"), "{}", r.causes);
        assert!(r.what_happened.contains("In the person's words:"), "the person's own words are kept");
    }

    #[test]
    fn an_edit_is_scrubbed_and_a_removed_excerpt_is_not_sent() {
        let ev = [item(Category::StartupCrash, "Launch record", "No start recorded")];
        let mut r = sample(&ev, None);
        r.edit("what_happened", "It broke. Mail me at jane@example.com", &Context::default()).unwrap();
        assert!(!r.render().1.contains("jane@"));
        r.set_excerpt_removed(&ev[0].id, true);
        assert!(!r.render().1.contains("No start recorded"));
        r.set_excerpt_removed(&ev[0].id, false);
        assert!(r.render().1.contains("No start recorded"));
        assert!(r.edit("nonsense", "x", &Context::default()).is_err());
    }

    #[test]
    fn a_huge_packet_is_shortened_before_review_with_a_note() {
        let big: String = (0..20_000).map(|i| format!("2026-10-22T08:43:10Z line {i}\n")).collect();
        let ev = [item(Category::StartupCrash, "Startup log", &big), item(Category::Richscribe, "RichScribe log", &big)];
        let r = sample(&ev, None);
        let (title, body) = r.render();
        assert!(body.len() <= BODY_MAX_BYTES, "{}", body.len());
        assert!(title.chars().count() <= TITLE_MAX_CHARS);
        assert!(body.ends_with("Report ID RD-7K3M-Q9VX"));
        assert!(body.contains("## What happened") && body.contains("## Versions"));
        assert!(!r.shortened.is_empty() && r.shortened[0].starts_with("Shortened to fit the size limit"), "{:?}", r.shortened);
        assert!(body.contains("line 19999"), "the newest lines are kept");
    }

    #[test]
    fn screenshots_end_the_body_with_their_count_and_staff_links_then_the_footer() {
        let mut r = sample(&[], None);
        r.screenshots = vec!["shot-1".into(), "shot-2".into()];
        let (_, body) = r.render();
        assert!(body.ends_with("Screenshots: 2, shared privately with RichOS support (staff sign-in required)\n1. https://report.richos.ceo/v1/reports/RD-7K3M-Q9VX/attachments/1\n2. https://report.richos.ceo/v1/reports/RD-7K3M-Q9VX/attachments/2\nSent with RichDoctor 1.0 · Report ID RD-7K3M-Q9VX"), "{body}");
        assert_eq!(r.problems(), vec![], "the links pass the last check");
        assert_eq!(ending("https://report.richos.ceo", "RD-7K3M-Q9VX", 0), footer("RD-7K3M-Q9VX"));
    }

    #[test]
    fn the_exact_view_is_the_title_and_the_body() {
        let r = sample(&[], None);
        let (t, b) = r.render();
        assert_eq!(r.exact(), format!("{t}\n\n{b}"));
    }
}
