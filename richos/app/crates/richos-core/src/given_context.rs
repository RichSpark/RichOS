//! Host context held by one provider conversation.
//!
//! Live ECS briefs follow the revision of the scoped projections. Other host records follow
//! their text. After compaction or a refused delivery the next prompt sends a fresh copy.

use std::collections::HashMap;

const STANDING_INSTRUCTIONS: &str = "standing-instructions";

/// The blocks given in full on this session since it started or last compacted, by name.
#[derive(Debug, Default)]
pub struct GivenContext {
    given: HashMap<String, String>,
    brief_revision: Option<String>,
    pending_brief_revision: Option<String>,
    compacted: bool,
}

impl GivenContext {
    pub fn brief_revision(&self) -> Option<&str> {
        self.brief_revision.as_deref()
    }

    pub fn continuity(&mut self, revision: Option<String>, text: String) -> String {
        if let Some(revision) = revision {
            self.pending_brief_revision = Some(revision);
            text
        } else {
            self.once("executive-continuity", &text)
        }
    }

    pub fn finish_turn(&mut self, delivered: bool) {
        if delivered {
            if let Some(revision) = self.pending_brief_revision.take() {
                self.brief_revision = Some(revision);
            }
        } else {
            self.brief_revision = None;
            self.pending_brief_revision = None;
            self.given.remove(STANDING_INSTRUCTIONS);
        }
    }

    pub fn compacted(&mut self) {
        self.forget();
        self.compacted = true;
    }

    pub fn needs_standing_instructions(&self) -> bool {
        self.compacted && !self.given.contains_key(STANDING_INSTRUCTIONS)
    }

    pub fn standing_instructions(&mut self, context: &str) -> String {
        self.once(STANDING_INSTRUCTIONS, context)
    }

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
        self.brief_revision = None;
        self.pending_brief_revision = None;
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
    fn scoped_revision_is_reused_after_delivery_and_forgotten_after_failure_or_compaction() {
        let mut given = GivenContext::default();
        assert_eq!(given.continuity(Some("first".into()), "live".into()), "live");
        assert_eq!(given.brief_revision(), None);
        given.finish_turn(true);
        assert_eq!(given.brief_revision(), Some("first"));
        assert_eq!(given.continuity(Some("first".into()), String::new()), "");
        given.finish_turn(true);
        assert_eq!(given.continuity(Some("second".into()), "changed".into()), "changed");
        given.finish_turn(false);
        assert_eq!(given.brief_revision(), None);
        given.continuity(Some("second".into()), "changed".into());
        given.finish_turn(true);
        given.forget();
        assert_eq!(given.brief_revision(), None);
    }

    #[test]
    fn blocks_are_remembered_by_name_and_an_empty_block_is_never_remembered() {
        let mut given = GivenContext::default();
        assert_eq!(given.once("a", ""), "");
        assert_eq!(given.once("a", "text"), "text");
        assert_eq!(given.once("b", "text"), "text", "another block's copy is not this one's");
    }
}
