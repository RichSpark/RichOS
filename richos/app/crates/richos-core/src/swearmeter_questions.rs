//! Counts of original question requests, independent of every delivery sink.
use crate::{questions::AnswerOrigin, swearmeter};
use std::collections::HashSet;

#[derive(Default)]
pub struct Projection {
    seen: HashSet<String>,
    pub known_composites: HashSet<String>,
    pub count: u64,
    pub unknown_inputs: u64,
    pub available: bool,
    pub revision: u64,
}

impl Projection {
    pub fn empty() -> Self { Self { available: true, ..Self::default() } }

    /// The original operation key is retained; no submitted text or excerpt is.
    pub fn record(&mut self, key: &str, origin: Option<AnswerOrigin>, text: &str) {
        if !self.seen.insert(key.to_string()) { return; }
        self.revision += 1;
        match origin {
            Some(AnswerOrigin::Direct) => self.count += swearmeter::count(text),
            Some(AnswerOrigin::Conversation) => {}
            _ if !text.is_empty() => self.unknown_inputs += 1,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_cannot_change_the_original_identity_or_origin() {
        let mut counts = Projection::empty();
        counts.record("entity:thread:request", Some(AnswerOrigin::Direct), "shit");
        counts.record("entity:thread:request", Some(AnswerOrigin::Conversation), "fuck fuck");
        assert_eq!(counts.count, 1);
        assert_eq!(counts.revision, 1);
        counts.record("entity:thread:new-send", Some(AnswerOrigin::Direct), "shit");
        assert_eq!(counts.count, 2);
    }

    #[test]
    fn empty_legacy_requests_are_verified_zero_and_model_text_is_never_human() {
        let mut counts = Projection::empty();
        counts.record("legacy-option", None, "");
        counts.record("model", Some(AnswerOrigin::Conversation), "fucking shit");
        assert_eq!(counts.count, 0);
        assert_eq!(counts.unknown_inputs, 0);
        counts.record("legacy-text", None, "shit");
        counts.record("unknown-future-origin", Some(AnswerOrigin::Unknown), "fuck");
        assert_eq!(counts.count, 0);
        assert_eq!(counts.unknown_inputs, 2);
    }
}
