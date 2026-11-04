//! Counts of original question requests, independent of every delivery sink.
use crate::{questions::AnswerOrigin, swearmeter};
use std::collections::HashSet;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Summary {
    count: u64, unknown_inputs: u64, available: bool, known_composites: HashSet<String>,
}

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

    pub fn summary(&self) -> Summary {
        Summary { count: self.count, unknown_inputs: self.unknown_inputs, available: self.available, known_composites: self.known_composites.clone() }
    }
    pub fn from_summary(summary: &Summary) -> Self {
        Self { count: summary.count, unknown_inputs: summary.unknown_inputs, available: summary.available,
            known_composites: summary.known_composites.clone(), ..Self::empty() }
    }
    pub fn fold_text(&mut self, origin: Option<AnswerOrigin>, text: &str) {
        self.revision += 1;
        match origin {
            Some(AnswerOrigin::Direct) => self.count += swearmeter::count(text),
            Some(AnswerOrigin::Conversation) => {}
            _ if !text.is_empty() => self.unknown_inputs += 1,
            _ => {}
        }
    }

    /// The original operation key is retained; no submitted text or excerpt is.
    pub fn record(&mut self, key: &str, origin: Option<AnswerOrigin>, text: &str) {
        if !self.seen.insert(key.to_string()) { return; }
        self.fold_text(origin, text);
    }
}

/// Startup needs original input requests, not generated result copies. Fold each
/// request while reading the existing operation map and release its text at once.
/// Duplicate keys make coverage unavailable rather than publishing an overcount.
pub(crate) struct OperationInputs {
    pub projection: Projection,
    pub represented: HashSet<String>,
}
impl Default for OperationInputs {
    fn default() -> Self { Self { projection: Projection::empty(), represented: HashSet::new() } }
}
impl<'de> serde::Deserialize<'de> for OperationInputs {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Inputs;
        impl<'de> serde::de::Visitor<'de> for Inputs {
            type Value = OperationInputs;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str("saved input operations") }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                #[derive(serde::Deserialize)]
                struct Input {
                    request: String,
                    #[serde(default)] origin: Option<AnswerOrigin>,
                    #[serde(rename = "result")] _result: serde::de::IgnoredAny,
                }
                let mut inputs = OperationInputs::default();
                let mut keys = HashSet::new();
                while let Some((key, operation)) = map.next_entry::<String, Input>()? {
                    if !keys.insert(key) { inputs.projection.available = false; }
                    match serde_json::from_str::<crate::questions::AnswerRequest>(&operation.request) {
                        Ok(request) => {
                            inputs.represented.insert(request.question_id);
                            inputs.projection.fold_text(operation.origin, &request.text);
                        }
                        Err(_) => inputs.projection.available = false,
                    }
                }
                Ok(inputs)
            }
        }
        deserializer.deserialize_map(Inputs)
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

