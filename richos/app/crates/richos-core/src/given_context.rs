//! **Host context the model already holds is not given to it again.**
//!
//! Every input to a lease (his message, a watcher notice, a message from another agent) gets
//! host context through the `richos-input-context` prompt hook: the executive-continuity block
//! and the company material. Both were given in full on every input, so one idle notice added
//! about 9,300 characters (about 2,300 tokens at chars/4) that the model already had. Measured
//! on Rich's session 4c627fc4 (2026-10-29T09:09Z to 2026-10-30T00:17Z): 406 injections, the
//! continuity block 2,404,070 characters with no two copies alike, the company material
//! 1,295,140 characters with every copy identical (follow-up "Rich's context fills with the
//! same injected blocks on every message", 2026-10-30).
//!
//! The continuity block is never byte-identical twice because the engine stamps each copy
//! with when it was compiled and the input's sequence number, so "send only when it changed"
//! is a line difference against the last full copy, not an equality test. Nothing here reads
//! the engine's format: lines are compared as lines.
//!
//! What this remembers is what the model can still see. A compaction replaces the
//! conversation with a summary and every earlier copy goes with it, so the reader forgets
//! everything at the provider's `compact_boundary` and the next input carries the full blocks
//! again. A new or resumed session is a new client and starts with nothing remembered.

use std::collections::HashMap;

/// The blocks given in full on this session since it started or last compacted, by name.
#[derive(Debug, Default)]
pub struct GivenContext {
    given: HashMap<String, String>,
}

/// Larger than this many line pairs and the difference is not computed: the full block is sent.
const DIFF_CELLS_LIMIT: usize = 4_000_000;

impl GivenContext {
    /// What to give the model for the block `name` whose current full text is `full`:
    ///
    /// - the full text, the first time on this session (or after a compaction), and whenever
    ///   the difference from the last full copy would be at least half as long as the block;
    /// - a one-line note when nothing changed;
    /// - otherwise only the lines that differ from the last full copy, which stays the base,
    ///   so each difference reads against one copy the model can see.
    ///
    /// The caller gives the result to the model; call it only once that is certain, or
    /// [`GivenContext::forget`] when it turns out the context was not delivered.
    pub fn render(&mut self, name: &str, full: &str) -> String {
        if full.trim().is_empty() {
            return full.to_string();
        }
        let Some(base) = self.given.get(name) else {
            self.given.insert(name.to_string(), full.to_string());
            return full.to_string();
        };
        if base == full {
            return format!("\n[{name}: unchanged since its last full copy in this conversation]\n");
        }
        if let Some(changes) = line_changes(base, full) {
            let short = format!(
                "\n[{name}: changed since its last full copy in this conversation; only these lines differ, every other line still holds]\n{changes}[end of {name} changes]\n"
            );
            if short.len() * 2 < full.len() {
                return short;
            }
        }
        self.given.insert(name.to_string(), full.to_string());
        full.to_string()
    }

    /// The model no longer holds any earlier copy: a compaction, or context that was rendered
    /// and then not delivered. The next input gets every block in full.
    pub fn forget(&mut self) {
        self.given.clear();
    }
}

/// The lines of `new` that differ from `old`, as hunks: the unchanged line before each hunk
/// (indented two spaces) when there is one, then `- ` for a line no longer there and `+ ` for a
/// line that is new. `None` when the blocks are too large to compare.
fn line_changes(old: &str, new: &str) -> Option<String> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let (n, m) = (a.len(), b.len());
    if (n + 1).saturating_mul(m + 1) > DIFF_CELLS_LIMIT {
        return None;
    }
    // Longest common subsequence, from the end, so the walk below goes forward.
    let width = m + 1;
    let mut lcs = vec![0u32; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i * width + j] = if a[i] == b[j] {
                lcs[(i + 1) * width + j + 1] + 1
            } else {
                lcs[(i + 1) * width + j].max(lcs[i * width + j + 1])
            };
        }
    }
    let mut out = String::new();
    let (mut i, mut j) = (0, 0);
    let mut last_same: Option<&str> = None;
    let mut in_hunk = false;
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            last_same = Some(a[i]);
            in_hunk = false;
            i += 1;
            j += 1;
            continue;
        }
        if !in_hunk {
            if let Some(line) = last_same {
                out.push_str("  ");
                out.push_str(line);
                out.push('\n');
            }
            in_hunk = true;
        }
        // A replaced line reads old first, then new.
        if i < n && (j == m || lcs[(i + 1) * width + j] >= lcs[i * width + j + 1]) {
            out.push_str("- ");
            out.push_str(a[i]);
            i += 1;
        } else {
            out.push_str("+ ");
            out.push_str(b[j]);
            j += 1;
        }
        out.push('\n');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(compiled: &str, sequence: u32, extra: &str) -> String {
        let mut text = format!(
            "\n<executive-continuity>\nheader\ncompiled_at={compiled}\n## Active context\n- session=s\n- authority=seq:{sequence}\n"
        );
        for section in 0..14 {
            text.push_str(&format!("## Section {section}\n- UNKNOWN (no authoritative record) {section}\n"));
        }
        text.push_str(extra);
        text.push_str("</executive-continuity>\n");
        text
    }

    #[test]
    fn the_first_copy_is_full_an_unchanged_one_is_a_line_and_a_stamped_one_is_its_changed_lines() {
        let mut given = GivenContext::default();
        let first = block("10:00", 4, "");
        assert_eq!(given.render("executive-continuity", &first), first);

        let same = given.render("executive-continuity", &first);
        assert!(same.len() < 100 && same.contains("unchanged"), "{same}");

        let stamped = block("10:05", 8, "");
        let changes = given.render("executive-continuity", &stamped);
        assert!(changes.len() * 2 < stamped.len(), "{changes}");
        assert!(changes.contains("- compiled_at=10:00\n+ compiled_at=10:05\n"), "{changes}");
        assert!(changes.contains("- - authority=seq:4\n") && changes.contains("+ - authority=seq:8\n"), "{changes}");
        assert!(changes.contains("  header\n"), "a hunk names the line it follows: {changes}");
        assert!(!changes.contains("Section 3"), "an unchanged line was given again: {changes}");

        // The base stays the last FULL copy, so a later difference reads against the copy
        // the model can see, not against a difference it was given.
        let later = given.render("executive-continuity", &block("10:09", 9, ""));
        assert!(later.contains("- compiled_at=10:00\n") && later.contains("+ compiled_at=10:09\n"), "{later}");
    }

    #[test]
    fn a_change_too_large_to_be_short_is_given_in_full_and_becomes_the_base() {
        let mut given = GivenContext::default();
        given.render("company", "{\"notes\":\"one\"}");
        let changed = "{\"notes\":\"two\"}";
        assert_eq!(given.render("company", changed), changed, "a one-line block that changed is given whole");
        assert!(given.render("company", changed).contains("unchanged"));
    }

    #[test]
    fn after_forgetting_every_block_is_given_in_full_again() {
        let mut given = GivenContext::default();
        let first = block("10:00", 4, "");
        given.render("executive-continuity", &first);
        given.render("company", "notes");
        given.forget();
        assert_eq!(given.render("executive-continuity", &first), first);
        assert_eq!(given.render("company", "notes"), "notes");
    }

    #[test]
    fn blocks_are_remembered_by_name_and_an_empty_block_is_never_remembered() {
        let mut given = GivenContext::default();
        assert_eq!(given.render("a", ""), "");
        assert_eq!(given.render("a", "text"), "text");
        assert_eq!(given.render("b", "text"), "text", "another block's copy is not this one's");
    }

    #[test]
    fn added_and_removed_lines_are_both_shown() {
        let old = "a\nb\nc\nd\n";
        let new = "a\nc\nd\ne\n";
        assert_eq!(line_changes(old, new).unwrap(), "  a\n- b\n  d\n+ e\n");
    }
}
