//! **Host context is given the terminal's way: at session start and after a compaction, not
//! with every input.**
//!
//! Every input to a lease (his message, a watcher notice, a message from another agent) goes
//! through the `richos-input-context` prompt hook, which can add host context: the
//! executive-continuity block, the Loro receipts and open questions, and the company material.
//! The terminal lead gets its continuity block when its session starts and after a compaction;
//! its per-prompt hook records and adds nothing (femcboost `scripts/ecs/claude_adapter.py`, no
//! `additionalContext`). The app gave the block in full once and then, on every later input, a
//! note or a line difference of it, with the receipts and open questions in full every time:
//! about 1.3M cost units in 37 hours of his use (richos-hq
//! `docs/research/2026-10-31-app-terminal-differences.md` finding 6).
//!
//! So a block is given once per conversation the model can see, and the rest only when what
//! it says changed:
//!
//! - [`GivenContext::once`] — the continuity block. The engine stamps every copy with when it
//!   was compiled and the input's sequence number, so it is never byte-identical twice and
//!   "when it changed" would be every input. As in the terminal, it is given again only after
//!   the model lost its copy.
//! - [`GivenContext::when_changed`] — the records (Loro receipts, open questions) and the
//!   company material: given in full when they differ from the copy the model holds, nothing
//!   otherwise.
//!
//! What this remembers is what the model can still see. A compaction replaces the
//! conversation with a summary and every earlier copy goes with it, so the reader forgets
//! everything at the provider's `compact_boundary` and the next input carries every block
//! again. A new or resumed session is a new client and starts with nothing remembered.

use std::collections::HashMap;

/// The blocks given in full on this session since it started or last compacted, by name.
#[derive(Debug, Default)]
pub struct GivenContext {
    given: HashMap<String, String>,
}

impl GivenContext {
    /// The block `name` the first time on this session (or after a compaction), and nothing
    /// again until [`GivenContext::forget`], whatever its text says by then.
    ///
    /// The caller gives the result to the model; call it only once that is certain, or
    /// [`GivenContext::forget`] when it turns out the context was not delivered.
    pub fn once(&mut self, name: &str, full: &str) -> String {
        if full.trim().is_empty() || self.given.contains_key(name) {
            return String::new();
        }
        self.given.insert(name.to_string(), full.to_string());
        full.to_string()
    }

    /// The block `name` in full when the model holds no copy of it or a different one, and
    /// nothing when its copy is this text. An empty block gives nothing and forgets the copy,
    /// so the same text coming back later is given again.
    pub fn when_changed(&mut self, name: &str, full: &str) -> String {
        if full.trim().is_empty() {
            self.given.remove(name);
            return String::new();
        }
        if self.given.get(name).is_some_and(|held| held == full) {
            return String::new();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn block(compiled: &str, sequence: u32) -> String {
        format!("\n<executive-continuity>\ncompiled_at={compiled}\n- authority=seq:{sequence}\n</executive-continuity>\n")
    }

    /// Fails on main `47e314bb6`, which gave a later input "[executive-continuity: changed
    /// since its last full copy ...]" and its changed lines.
    #[test]
    fn the_continuity_block_is_given_once_and_a_restamped_copy_adds_nothing() {
        let mut given = GivenContext::default();
        let first = block("10:00", 4);
        assert_eq!(given.once("executive-continuity", &first), first);
        assert_eq!(given.once("executive-continuity", &first), "", "an unchanged copy was given again");
        assert_eq!(given.once("executive-continuity", &block("10:05", 8)), "", "a restamped copy was given again");
    }

    /// Fails on main `47e314bb6`, which appended the receipts and open questions to every input.
    #[test]
    fn records_are_given_when_they_change_and_nothing_while_they_do_not() {
        let mut given = GivenContext::default();
        let one = "{\"event\":\"question_records\",\"questions\":[1]}";
        assert_eq!(given.when_changed("records", one), one);
        assert_eq!(given.when_changed("records", one), "");
        let two = "{\"event\":\"question_records\",\"questions\":[1,2]}";
        assert_eq!(given.when_changed("records", two), two, "a changed record was not given");
        assert_eq!(given.when_changed("records", two), "");
        // None left: nothing to give, and the same record coming back is given again.
        assert_eq!(given.when_changed("records", ""), "");
        assert_eq!(given.when_changed("records", two), two);
    }

    #[test]
    fn after_forgetting_every_block_is_given_in_full_again() {
        let mut given = GivenContext::default();
        let first = block("10:00", 4);
        given.once("executive-continuity", &first);
        given.when_changed("company", "notes");
        given.forget();
        assert_eq!(given.once("executive-continuity", &first), first);
        assert_eq!(given.when_changed("company", "notes"), "notes");
    }

    #[test]
    fn blocks_are_remembered_by_name_and_an_empty_block_is_never_remembered() {
        let mut given = GivenContext::default();
        assert_eq!(given.once("a", ""), "");
        assert_eq!(given.once("a", "text"), "text");
        assert_eq!(given.once("b", "text"), "text", "another block's copy is not this one's");
    }
}
